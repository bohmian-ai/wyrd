//! Portable run snapshot ownership and size accounting.
//!
//! [`RunLedger`] owns the [`WorkflowRun`] snapshot for one execution. It
//! applies step transitions, selects binding values from completed results,
//! and terminalizes the run with the exact status/field invariants. When a
//! run-size limit is set, the ledger reserves room for the complete
//! payload-free terminal snapshot plus one bounded error per step and for the
//! run before any payload is admitted, so cancellation, timeout, and failure
//! can always be recorded.

use std::collections::BTreeMap;

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use wyrd_spec::card::workflow::{
    WORKFLOW_RUN_ERROR_MAX_BYTES, WorkflowBinding, WorkflowBindingSource, WorkflowRun,
    WorkflowRunError, WorkflowRunStatus, WorkflowStepResult, WorkflowStepStatus, jcs_len,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::WorkflowRunId;
use wyrd_spec::reference::CardRef;

use crate::attempt::StepPayload;
use crate::error::WorkflowResult;
use crate::plan::ExecutionPlan;

/// JCS size of `null`, already counted in the payload-free snapshot for each
/// absent error.
const NULL_BYTES: usize = 4;

/// How a run ended before terminalization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunEnding {
    /// Every scheduled step settled.
    Settled,
    /// The caller cancelled the run.
    Cancelled,
    /// The total run deadline expired.
    TimedOut,
}

/// Owner of one run's snapshot and payload budget.
pub(crate) struct RunLedger {
    /// Snapshot under construction.
    run: WorkflowRun,
    /// Step IDs in plan order.
    order: Vec<String>,
    /// Bytes payloads and outputs may use, when the run size is capped.
    payload_budget: Option<usize>,
    /// Bytes admitted payloads already use.
    payload_used: usize,
    /// Aggregate-size failure, which decides the run error when present.
    size_error: Option<WorkflowRunError>,
}

impl RunLedger {
    /// Create the queued snapshot and check the terminal reserve.
    ///
    /// # Errors
    ///
    /// Returns `WYRD_WORKFLOW_413_GRAPH_TOO_LARGE` when the payload-free
    /// terminal snapshot plus bounded errors cannot fit `max_run_bytes`.
    pub(crate) fn new(
        run_id: WorkflowRunId,
        workflow: Option<CardRef>,
        plan: &ExecutionPlan,
        max_run_bytes: Option<usize>,
    ) -> WorkflowResult<Self> {
        let order: Vec<String> = plan.steps.iter().map(|step| step.id.clone()).collect();
        let run = WorkflowRun {
            run_id,
            workflow,
            status: WorkflowRunStatus::Queued,
            outputs: BTreeMap::new(),
            steps: order
                .iter()
                .map(|id| (id.clone(), WorkflowStepResult::pending()))
                .collect(),
            created_at: Utc::now(),
            started_at: None,
            ended_at: None,
            error: None,
        };
        let payload_budget = match max_run_bytes {
            None => None,
            Some(limit) => {
                let reserve = terminal_reserve(&run, plan);
                if reserve > limit {
                    return Err(WyrdError::WorkflowGraphTooLarge {
                        message: format!(
                            "terminal run snapshot needs {reserve} bytes; the limit is {limit}"
                        ),
                        details: serde_json::json!({ "limit": limit }),
                    }
                    .into());
                }
                Some(limit - reserve)
            }
        };
        Ok(Self {
            run,
            order,
            payload_budget,
            payload_used: 0,
            size_error: None,
        })
    }

    /// Run identity.
    pub(crate) fn run_id(&self) -> &WorkflowRunId {
        &self.run.run_id
    }

    /// Mark the run started.
    pub(crate) fn start(&mut self) {
        self.run.status = WorkflowRunStatus::Running;
        self.run.started_at = Some(Utc::now());
    }

    /// Mark the step at plan index `index` running.
    pub(crate) fn step_started(&mut self, index: usize) {
        if let Some(step) = self.step_mut(index) {
            step.status = WorkflowStepStatus::Running;
            step.started_at = Some(Utc::now());
        }
    }

    /// Record a succeeded step, admitting its payload against the run budget.
    ///
    /// Returns `false` when the payload would exceed the run budget; the
    /// payload is discarded, the step fails with
    /// `WYRD_WORKFLOW_413_RUN_TOO_LARGE`, and that error becomes the run error.
    pub(crate) fn step_succeeded(
        &mut self,
        index: usize,
        payload: StepPayload,
        attempts: u32,
    ) -> bool {
        let charged = payload.charged_bytes();
        if !self.admit(charged) {
            let error = self.run_too_large();
            self.step_failed(index, error, attempts);
            return false;
        }
        if let Some(step) = self.step_mut(index) {
            step.status = WorkflowStepStatus::Succeeded;
            step.text = payload.text;
            step.structured_output = payload.structured;
            step.attempts = attempts;
            step.ended_at = Some(Utc::now());
        }
        true
    }

    /// Record a failed step with its bounded error.
    pub(crate) fn step_failed(&mut self, index: usize, error: WorkflowRunError, attempts: u32) {
        if let Some(step) = self.step_mut(index) {
            step.status = WorkflowStepStatus::Failed;
            step.error = Some(error.bounded());
            step.attempts = attempts;
            step.started_at.get_or_insert_with(Utc::now);
            step.ended_at = Some(Utc::now());
        }
    }

    /// Record an interrupted active step.
    pub(crate) fn step_cancelled(&mut self, index: usize, attempts: u32) {
        if let Some(step) = self.step_mut(index) {
            step.status = WorkflowStepStatus::Cancelled;
            step.attempts = attempts;
            step.ended_at = Some(Utc::now());
        }
    }

    /// Select the value a binding names from the input and succeeded steps.
    ///
    /// Text selects a step's text; structured selects its whole structured
    /// value or a nested object field. Returns `None` when the step has not
    /// succeeded or the field is absent.
    pub(crate) fn select(
        &self,
        input: &BTreeMap<String, Value>,
        binding: &WorkflowBinding,
    ) -> Option<Value> {
        match binding.source() {
            WorkflowBindingSource::Input(name) => input.get(name).cloned(),
            WorkflowBindingSource::StepText(step) => self
                .succeeded(step)?
                .text
                .as_ref()
                .map(|text| Value::String(text.clone())),
            WorkflowBindingSource::StepStructured { step, path } => {
                let mut value = self.succeeded(step)?.structured_output.as_ref()?;
                if !path.is_empty() {
                    for field in path.split('.') {
                        value = value.as_object()?.get(field)?;
                    }
                }
                Some(value.clone())
            }
        }
    }

    /// Terminalize the snapshot.
    ///
    /// Pending steps become `unstarted`. Cancellation and the total deadline
    /// decide the status first, then an aggregate-size failure, then the
    /// failed step earliest in plan order (stage, then step ID). Otherwise
    /// every declared output is projected with its JSON type; a missing
    /// output field or an over-budget projection fails the run.
    pub(crate) fn finish(mut self, ending: RunEnding, plan: &ExecutionPlan) -> WorkflowRun {
        for step in self.run.steps.values_mut() {
            if matches!(
                step.status,
                WorkflowStepStatus::Pending | WorkflowStepStatus::Running
            ) {
                step.status = WorkflowStepStatus::Unstarted;
                step.started_at = None;
                step.attempts = 0;
            }
        }
        self.run.ended_at = Some(Utc::now());
        match ending {
            RunEnding::Cancelled => {
                self.run.status = WorkflowRunStatus::Cancelled;
                return self.run;
            }
            RunEnding::TimedOut => {
                self.run.status = WorkflowRunStatus::TimedOut;
                self.run.error = Some(WorkflowRunError::from_wyrd(
                    &WyrdError::WorkflowRunTimeout {
                        message: "workflow run exceeded its total deadline".to_owned(),
                        details: serde_json::json!({}),
                    },
                ));
                return self.run;
            }
            RunEnding::Settled => {}
        }
        let primary = self.size_error.take().or_else(|| {
            self.order
                .iter()
                .filter_map(|id| self.run.steps.get(id))
                .find_map(|step| step.error.clone())
        });
        if let Some(error) = primary {
            return self.fail(error);
        }
        let mut outputs = BTreeMap::new();
        for (name, binding) in &plan.outputs {
            let Some(value) = self.select(&plan.input, binding) else {
                let error = WorkflowRunError::from_wyrd(&WyrdError::WorkflowMissingParameter {
                    message: format!("output '{name}' selects a missing value"),
                    details: serde_json::json!({ "field": format!("outputs.{name}") }),
                });
                return self.fail(error);
            };
            outputs.insert(name.clone(), value);
        }
        if !self.admit(jcs_len(&outputs)) {
            let error = self.run_too_large();
            return self.fail(error);
        }
        self.run.outputs = outputs;
        self.run.status = WorkflowRunStatus::Succeeded;
        self.run
    }

    /// Fail the run with `error` and no outputs.
    fn fail(mut self, error: WorkflowRunError) -> WorkflowRun {
        self.run.status = WorkflowRunStatus::Failed;
        self.run.outputs.clear();
        self.run.error = Some(error);
        self.run
    }

    /// Charge `bytes` against the payload budget when it fits.
    fn admit(&mut self, bytes: usize) -> bool {
        match self.payload_budget {
            None => true,
            Some(budget) => {
                let used = self.payload_used.saturating_add(bytes);
                if used > budget {
                    false
                } else {
                    self.payload_used = used;
                    true
                }
            }
        }
    }

    /// Record and return the aggregate-size error.
    fn run_too_large(&mut self) -> WorkflowRunError {
        let error = WorkflowRunError::from_wyrd(&WyrdError::WorkflowRunTooLarge {
            message: "workflow run snapshot would exceed its size limit".to_owned(),
            details: serde_json::json!({}),
        });
        self.size_error.get_or_insert_with(|| error.clone());
        error
    }

    /// Borrow the result of a succeeded step.
    fn succeeded(&self, step: &str) -> Option<&WorkflowStepResult> {
        self.run
            .steps
            .get(step)
            .filter(|result| result.status == WorkflowStepStatus::Succeeded)
    }

    /// Mutably borrow the step at plan index `index`.
    fn step_mut(&mut self, index: usize) -> Option<&mut WorkflowStepResult> {
        let id = self.order.get(index)?;
        self.run.steps.get_mut(id)
    }
}

/// Bytes reserved for the payload-free terminal snapshot.
///
/// The worst case uses nine-character statuses, every timestamp present with
/// full nanosecond precision, and the maximum attempt count per step, then
/// adds one maximal error for the run and for every step beyond the `null`
/// already counted.
fn terminal_reserve(run: &WorkflowRun, plan: &ExecutionPlan) -> usize {
    let widest = widest_timestamp();
    let mut skeleton = run.clone();
    skeleton.status = WorkflowRunStatus::Succeeded;
    skeleton.started_at = Some(widest);
    skeleton.ended_at = Some(widest);
    skeleton.created_at = widest;
    for planned in &plan.steps {
        if let Some(step) = skeleton.steps.get_mut(&planned.id) {
            step.status = WorkflowStepStatus::Succeeded;
            step.attempts = planned.max_retries.saturating_add(1);
            step.started_at = Some(widest);
            step.ended_at = Some(widest);
        }
    }
    let errors = (plan.steps.len() + 1).saturating_mul(WORKFLOW_RUN_ERROR_MAX_BYTES - NULL_BYTES);
    jcs_len(&skeleton).saturating_add(errors)
}

/// A four-digit-year timestamp with nine fractional digits, the widest form
/// a run timestamp serializes to.
fn widest_timestamp() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(9999, 12, 31, 23, 59, 59)
        .single()
        .and_then(|time| time.checked_add_signed(chrono::Duration::nanoseconds(999_999_999)))
        .unwrap_or_else(Utc::now)
}
