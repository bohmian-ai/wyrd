//! `EvalSpec` — the typed spec body for the `Eval` card kind.
//!
//! Envelope-level fields (`metadata.space`, `metadata.name`,
//! `metadata.version`, `apiVersion`) live on the outer `Card`, not here.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::envelope::CardKind;
use crate::error::WyrdError;
use crate::reference::CardRef;

use super::ids::{JsonPath, TaskId};
use super::operator::{ComparisonOperator, validate_regex_pattern};
use super::plan::{DagError, ExecutionPlan, validate_dag};
use super::result::{EvalContextCapture, EvalPassGate};
use super::task::EvalTask;
use super::workflow::Workflow;

/// Maximum number of tasks allowed in an [`EvalSpec`] task DAG.
///
/// Guards against unbounded O(V+E) allocation in DAG validation before any
/// auth or quota check. Mirrors `MAX_SCENARIOS_PER_COLLECTION` on the scenario
/// side.
pub const MAX_EVAL_TASKS: usize = 512;

/// The typed spec body for an `Eval` card.
///
/// This struct does not carry a discriminator; the outer `CardBody` enum
/// carries the envelope-level `kind: Eval` and `spec: { ... }` wire shape.
///
/// Authors declare:
/// - `subject_ref` — canonical card this rubric judges per doctrine #3 and
///   the `ObservationCriteria.subject_refs` precedent.
/// - `dataset` — source data, typically a `Data` card with eval scenarios.
/// - `tasks` — the rubric DAG.
/// - `workflow` — optional declared workflow shape.
/// - `sampling` — optional production sampling policy.
/// - `pass_gate` — optional workflow-level pass criterion.
/// - `context_capture` — durable storage policy for `AssertionResult::actual`.
///
/// This struct does not carry envelope identity, tenant partitioning, triggers,
/// or alert dispatch. Alert wiring is deferred until the drift primitives and
/// secret reference model are locked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvalSpec {
    /// Declarative subject — the card whose runs this rubric judges.
    ///
    /// When `Some`, registration can derive an `evaluates` relationship.
    /// Runtime records may still supply the subject when this is `None`.
    ///
    /// **Presence rule (D8).** The struct keeps `Option<CardRef>`, but
    /// registry validation at `wyrd apply` rejects an Eval card with
    /// `subject_ref = None` whenever the card receives online observations
    /// (`run.observe.eval(...)` needs the identity chain). It may be `None`
    /// only for pure-offline rubric cards driven entirely by datasets and
    /// scenarios. The validator itself ships with the registry surface; this
    /// field documents the rule the validator enforces, consistent with the
    /// kind-restriction note on `validate()` below.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subject_ref: Option<CardRef>,

    /// Source data for offline runs.
    ///
    /// Validation enforces `dataset.kind == Data`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset: Option<DatasetRef>,

    /// Task DAG keyed by `TaskId`.
    pub tasks: BTreeMap<TaskId, EvalTask>,

    /// Optional declared workflow shape used as a path validation hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<Workflow>,

    /// Optional sampling policy applied at the record-ingest boundary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampling: Option<EvalSampling>,

    /// Optional workflow-level pass gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pass_gate: Option<EvalPassGate>,

    /// How `vala-eval` captures extracted values in [`super::AssertionResult::actual`].
    ///
    /// Set to [`EvalContextCapture::Redact`] when this eval runs over
    /// production traffic where context fields may contain PII. Absent means
    /// [`EvalContextCapture::Full`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_capture: Option<EvalContextCapture>,
}

impl EvalSpec {
    /// Construct an eval spec after validating its task DAG.
    ///
    /// The returned spec has no subject, dataset, workflow, sampling policy,
    /// pass gate, or context-capture override. Call [`EvalSpec::validate`]
    /// after mutating optional fields.
    ///
    /// # Errors
    /// Returns [`EvalSpecError::Wyrd`] when a task map key does not match its
    /// inner id, or [`EvalSpecError::Dag`] when the task map is not a DAG.
    pub fn new(tasks: BTreeMap<TaskId, EvalTask>) -> Result<Self, EvalSpecError> {
        if tasks.len() > MAX_EVAL_TASKS {
            return Err(EvalSpecError::Wyrd(WyrdError::Validation {
                message: format!(
                    "eval_spec.tasks count {} exceeds MAX_EVAL_TASKS {MAX_EVAL_TASKS}",
                    tasks.len()
                ),
                details: serde_json::Value::Null,
            }));
        }
        validate_task_keys(&tasks).map_err(EvalSpecError::Wyrd)?;
        validate_dag(&tasks).map_err(EvalSpecError::Dag)?;
        Ok(Self {
            subject_ref: None,
            dataset: None,
            tasks,
            workflow: None,
            sampling: None,
            pass_gate: None,
            context_capture: None,
        })
    }

    /// Validate the task DAG, task-level validators, dataset, pass gate, and
    /// sampling policy.
    ///
    /// `subject_ref` kind restrictions and the presence rule are intentionally
    /// deferred to registry validators: the allowlist is not locked here, and
    /// the D8 presence rule (online-observation Eval cards require
    /// `subject_ref`) is enforced at `wyrd apply` against the registered card
    /// graph, not in pure-spec validation.
    ///
    /// # Errors
    /// Returns [`EvalSpecError`] when DAG validation fails or a nested Wyrd
    /// validation rule rejects the spec.
    pub fn validate(&self) -> Result<(), EvalSpecError> {
        if self.tasks.len() > MAX_EVAL_TASKS {
            return Err(EvalSpecError::Wyrd(WyrdError::Validation {
                message: format!(
                    "eval_spec.tasks count {} exceeds MAX_EVAL_TASKS {MAX_EVAL_TASKS}",
                    self.tasks.len()
                ),
                details: serde_json::Value::Null,
            }));
        }
        validate_task_keys(&self.tasks).map_err(EvalSpecError::Wyrd)?;
        validate_dag(&self.tasks).map_err(EvalSpecError::Dag)?;

        for task in self.tasks.values() {
            if let EvalTask::LlmJudge(task) = task {
                task.validate().map_err(EvalSpecError::Wyrd)?;
            }
            if let Some(condition) = task.condition() {
                condition.validate().map_err(EvalSpecError::Wyrd)?;
            }
            validate_task_operator(task).map_err(EvalSpecError::Wyrd)?;
        }

        if let Some(dataset) = &self.dataset {
            dataset.validate().map_err(EvalSpecError::Wyrd)?;
        }
        if let Some(pass_gate) = &self.pass_gate {
            pass_gate.validate().map_err(EvalSpecError::Wyrd)?;
        }
        if let Some(sampling) = &self.sampling {
            sampling.validate().map_err(EvalSpecError::Wyrd)?;
        }
        Ok(())
    }

    /// Return the topologically sorted execution plan.
    ///
    /// This method calls DAG validation but not full nested spec validation.
    ///
    /// # Errors
    /// Returns [`DagError`] when the task map is not a DAG.
    pub fn execution_plan(&self) -> Result<ExecutionPlan, DagError> {
        validate_dag(&self.tasks)
    }
}

/// `EvalSpec` validation aggregate.
#[derive(Debug, thiserror::Error)]
pub enum EvalSpecError {
    /// The task map failed DAG validation.
    #[error("dag validation failed: {0}")]
    Dag(#[from] DagError),
    /// A nested Wyrd validation rule failed.
    #[error("validation failed: {0}")]
    Wyrd(#[from] WyrdError),
}

impl From<EvalSpecError> for WyrdError {
    fn from(error: EvalSpecError) -> Self {
        match error {
            EvalSpecError::Dag(error) => error.into(),
            EvalSpecError::Wyrd(error) => error,
        }
    }
}

fn validate_task_operator(task: &EvalTask) -> Result<(), WyrdError> {
    let operator = match task {
        EvalTask::Assertion(t) => &t.operator,
        EvalTask::LlmJudge(t) => &t.operator,
        EvalTask::TraceAssertion(t) => &t.operator,
        EvalTask::AgentAssertion(t) => &t.operator,
    };
    match operator {
        ComparisonOperator::MatchesRegex { pattern }
        | ComparisonOperator::NotMatchesRegex { pattern } => validate_regex_pattern(pattern),
        _ => Ok(()),
    }
}

fn validate_task_keys(tasks: &BTreeMap<TaskId, EvalTask>) -> Result<(), WyrdError> {
    for (key, task) in tasks {
        if key != task.id() {
            return Err(WyrdError::Validation {
                message: format!(
                    "eval_spec.tasks key {:?} must match inner task id {:?}",
                    key.as_str(),
                    task.id().as_str()
                ),
                details: serde_json::json!({
                    "field": "tasks",
                    "key": key.as_str(),
                    "task_id": task.id().as_str(),
                }),
            });
        }
    }
    Ok(())
}

/// Thin `CardRef` wrapper constraining the referenced kind to `Data`.
///
/// Deserialization routes through [`DatasetRef::new`] so kind enforcement fires
/// at the wire boundary, consistent with [`super::ids::TaskId`] and
/// [`super::ids::JsonPath`].
#[derive(Debug, Clone, PartialEq, Serialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct DatasetRef(pub CardRef);

impl<'de> Deserialize<'de> for DatasetRef {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let card_ref = CardRef::deserialize(deserializer)?;
        Self::new(card_ref).map_err(serde::de::Error::custom)
    }
}

impl DatasetRef {
    /// Construct a dataset reference, rejecting non-`Data` card kinds.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when `card_ref.kind` is not
    /// [`CardKind::Data`].
    pub fn new(card_ref: CardRef) -> Result<Self, WyrdError> {
        if card_ref.kind != CardKind::Data {
            return Err(Self::kind_mismatch(&card_ref.kind));
        }
        Ok(Self(card_ref))
    }

    /// Re-validate after deserialization.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when the wrapped reference is not a
    /// `Data` card reference.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.0.kind != CardKind::Data {
            return Err(Self::kind_mismatch(&self.0.kind));
        }
        Ok(())
    }

    /// Borrow the wrapped card reference.
    #[must_use]
    pub fn as_card_ref(&self) -> &CardRef {
        &self.0
    }

    fn kind_mismatch(kind: &CardKind) -> WyrdError {
        WyrdError::ValaEvalRefKindMismatch {
            message: format!("dataset_ref must reference a Data card; got {kind:?}"),
            details: serde_json::json!({
                "field": "dataset",
                "expected": "Data",
                "got": format!("{kind:?}"),
            }),
        }
    }
}

/// Sampling policy applied at the record-ingest boundary.
///
/// `vala-eval` enforces the policy when records are admitted for evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EvalSampling {
    /// Probabilistic selection.
    Ratio {
        /// Selection ratio in `[0.0, 1.0]`.
        ratio: f64,
    },
    /// Deterministic 1-in-N selection by hashing a stable key path.
    DeterministicByHash {
        /// JSONPath selecting the stable key to hash.
        key_path: JsonPath,
        /// Positive modulus for bucket assignment.
        modulus: u32,
        /// Selected bucket. Must be less than `modulus`.
        bucket: u32,
    },
    /// Every Nth record.
    EveryNth {
        /// Positive interval. Must be at least 1.
        n: u32,
    },
}

impl EvalSampling {
    /// Validate sampling bounds.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when a ratio is out of range, a
    /// deterministic hash modulus is zero, a bucket is outside the modulus, or
    /// an every-Nth interval is zero.
    pub fn validate(&self) -> Result<(), WyrdError> {
        match self {
            EvalSampling::Ratio { ratio } => {
                if !(0.0..=1.0).contains(ratio) || ratio.is_nan() {
                    return Err(WyrdError::Validation {
                        message: format!("eval_sampling.ratio must be in [0.0, 1.0]; got {ratio}"),
                        details: serde_json::Value::Null,
                    });
                }
            }
            EvalSampling::DeterministicByHash {
                modulus, bucket, ..
            } => {
                if *modulus == 0 {
                    return Err(WyrdError::Validation {
                        message: "eval_sampling.modulus must be >= 1".to_string(),
                        details: serde_json::Value::Null,
                    });
                }
                if bucket >= modulus {
                    return Err(WyrdError::Validation {
                        message: format!(
                            "eval_sampling.bucket {bucket} must be < modulus {modulus}"
                        ),
                        details: serde_json::Value::Null,
                    });
                }
            }
            EvalSampling::EveryNth { n } => {
                if *n == 0 {
                    return Err(WyrdError::Validation {
                        message: "eval_sampling.n must be >= 1".to_string(),
                        details: serde_json::Value::Null,
                    });
                }
            }
        }
        Ok(())
    }
}
