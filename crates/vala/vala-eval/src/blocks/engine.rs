//! The Verifier driver: data preparation, then judgments, then one verdict.
//!
//! [`VerificationEngine::run`] walks the stages [`VerifierSpec::plan`]
//! derives from references and `depends_on`. Within a stage, every runnable
//! step's action runs concurrently under the engine's concurrency bound;
//! after the stage's actions settle, their step requirements are judged, so a
//! dependent never starts before its upstream action and step requirements
//! finish. Whole-Verifier requirements run once after the last stage. Each
//! requirement is judged exactly once and reaches the `decide` rule once.
//!
//! Bindings are resolved against the [`Scope`] before any executor runs.
//! A value that cannot be supplied never reaches an executor: the dependent
//! step is skipped, or the requirement takes the outcome of the gap's cause,
//! so a failure cannot hide behind a skipped dependency. Executor and
//! check-isolated failures become step failures or inconclusive requirements,
//! never subject failures; only the execution-wide deadline and orchestration
//! faults surface as [`EngineError`].

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{Instant, timeout_at};
use tracing::Instrument as _;

use wyrd_spec::card::blocks::{Action, Check, Decide, Requirement, Step, TraceCheck, VerifierSpec};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::SlotName;
use wyrd_spec::vala::eval::result::EvalContextCapture;
use wyrd_spec::vala::eval::{ExecutionPlan, TaskId};
use wyrd_spec::verification::VerificationVerdict;
use wyrd_spec::verification::execution::{
    CallProvenance, Citation, InputDeclaration, RequirementDetail, RequirementOutcome,
    RequirementResult, StepResult, StepStatus, VerificationOutcome,
};

use super::drift::DriftChecks;
use super::scope::{Cause, Missing, Scope, Settled};
use super::{
    ActionExecutor, ActionOutput, BlockCall, BlockError, CallInput, CallValue, CheckExecutor,
    CheckOutput, Position,
};
use crate::context::extract_jsonpath_from;
use crate::error::EvalExecError;
use crate::operators;
use crate::results::capture_actual;
use crate::tasks::assertion::{Compared, evaluate_item_mode, evaluate_single};
use crate::tasks::trace::project_spans;
use crate::trace_source::TraceSource;

/// An execution-wide failure: no verdict was produced.
#[derive(Debug, Error)]
pub enum EngineError {
    /// The Verifier's step graph cannot be planned.
    #[error("the Verifier cannot be planned: {0}")]
    Plan(WyrdError),
    /// The execution deadline expired; unfinished work was cancelled.
    #[error("the verification deadline expired")]
    DeadlineExceeded,
    /// A requirement's actual value could not be captured.
    #[error("a requirement actual value cannot be captured: {0}")]
    Capture(#[from] EvalExecError),
    /// A block task panicked or was aborted.
    #[error("a verification block task stopped unexpectedly: {0}")]
    Join(String),
}

/// The engine of one Verifier execution, composed from injected capabilities.
///
/// Built per execution with executors that carry the accepted caller
/// authority. Cloning shares the same executors and concurrency bound.
#[derive(Clone)]
pub struct VerificationEngine {
    /// Runs step actions.
    actions: Arc<dyn ActionExecutor>,
    /// Runs Agent and code checks.
    checks: Arc<dyn CheckExecutor>,
    /// Reads declared trace evidence for `trace` checks.
    traces: Arc<dyn TraceSource>,
    /// Scheduled evidence for `drift` checks.
    drift: Arc<DriftChecks>,
    /// Bounds concurrently running actions and checks.
    permits: Arc<Semaphore>,
}

impl VerificationEngine {
    /// An engine running at most `concurrency` blocks at once, with no
    /// scheduled drift evidence.
    #[must_use]
    pub fn new(
        actions: Arc<dyn ActionExecutor>,
        checks: Arc<dyn CheckExecutor>,
        traces: Arc<dyn TraceSource>,
        concurrency: NonZeroUsize,
    ) -> Self {
        Self {
            actions,
            checks,
            traces,
            drift: Arc::new(DriftChecks::default()),
            permits: Arc::new(Semaphore::new(concurrency.get())),
        }
    }

    /// Use `drift` as the scheduled evidence of `drift` checks.
    #[must_use]
    pub fn with_drift(mut self, drift: DriftChecks) -> Self {
        self.drift = Arc::new(drift);
        self
    }

    /// Execute `spec` over the pinned `evidence` before `deadline`.
    ///
    /// Returns the verdict, every requirement in plan order (step
    /// requirements by step, then whole-Verifier requirements in declaration
    /// order), every step in plan order, and the evidence unchanged.
    ///
    /// # Errors
    /// Returns [`EngineError::Plan`] for an unplannable graph,
    /// [`EngineError::DeadlineExceeded`] when `deadline` passes first,
    /// [`EngineError::Capture`] when an actual value cannot be hashed, and
    /// [`EngineError::Join`] when a block task panics.
    ///
    /// # Cancellation
    /// Dropping the future, or the deadline expiring, aborts every running
    /// action and check; executors release their resources on drop and no
    /// partial result is returned.
    pub async fn run(
        &self,
        spec: Arc<VerifierSpec>,
        evidence: BTreeMap<SlotName, InputDeclaration>,
        deadline: Instant,
    ) -> Result<VerificationOutcome, EngineError> {
        let plan = spec.plan().map_err(EngineError::Plan)?;
        let (steps, requirements) =
            timeout_at(deadline, self.drive(&spec, &plan, &evidence, deadline))
                .await
                .map_err(|_| EngineError::DeadlineExceeded)??;
        Ok(VerificationOutcome {
            verdict: decide(
                spec.decide,
                requirements.iter().map(|result| result.outcome),
            ),
            requirements,
            steps,
            evidence,
        })
    }

    /// Run every stage, then the whole-Verifier requirements.
    ///
    /// # Errors
    /// Returns [`EngineError::Capture`] or [`EngineError::Join`].
    async fn drive(
        &self,
        spec: &Arc<VerifierSpec>,
        plan: &ExecutionPlan,
        evidence: &BTreeMap<SlotName, InputDeclaration>,
        deadline: Instant,
    ) -> Result<(Vec<StepResult>, Vec<RequirementResult>), EngineError> {
        let positions: HashMap<&TaskId, usize> = spec
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| (&step.id, index))
            .collect();
        let mut scope = Scope::new(spec, evidence);
        let mut steps = Vec::with_capacity(spec.steps.len());
        let mut requirements = Vec::new();
        for stage in &plan.stages {
            let stage: Vec<usize> = stage
                .tasks
                .iter()
                .map(|id| {
                    *positions
                        .get(id)
                        .expect("the plan names only declared steps")
                })
                .collect();
            for (index, result, settled) in self.run_steps(spec, &scope, &stage, deadline).await? {
                scope.settle(spec.steps[index].id.clone(), settled);
                steps.push(result);
            }
            let keys = stage.iter().flat_map(|&index| {
                (0..spec.steps[index].requirements.len()).map(move |at| (Some(index), at))
            });
            for result in self.run_requirements(spec, &scope, keys, deadline).await? {
                scope.record_verdict(result.id.clone(), result.outcome, result.reason.clone());
                requirements.push(result);
            }
        }
        let keys = (0..spec.requirements.len()).map(|at| (None, at));
        requirements.extend(self.run_requirements(spec, &scope, keys, deadline).await?);
        Ok((steps, requirements))
    }

    /// Run one stage's steps concurrently, returning each step's result and
    /// settled state in stage order.
    ///
    /// # Errors
    /// Returns [`EngineError::Join`] when a step task panics.
    async fn run_steps(
        &self,
        spec: &Arc<VerifierSpec>,
        scope: &Scope<'_>,
        stage: &[usize],
        deadline: Instant,
    ) -> Result<Vec<(usize, StepResult, Settled)>, EngineError> {
        let mut settled = HashMap::new();
        let mut running = JoinSet::new();
        for &index in stage {
            let step = &spec.steps[index];
            match prepare(scope, step) {
                Err(gap) => {
                    settled.insert(index, skipped(step, gap));
                }
                Ok(bindings) => {
                    let engine = self.clone();
                    let spec = Arc::clone(spec);
                    running.spawn(
                        async move {
                            let step = &spec.steps[index];
                            let attempted = engine.attempt(step, &bindings, deadline).await;
                            (index, attempted.settle(step, bindings.sources()))
                        }
                        .instrument(tracing::Span::current()),
                    );
                }
            }
        }
        while let Some(joined) = running.join_next().await {
            let (index, done) = joined.map_err(|error| EngineError::Join(error.to_string()))?;
            settled.insert(index, done);
        }
        Ok(stage
            .iter()
            .filter_map(|index| {
                let (result, state) = settled.remove(index)?;
                Some((*index, result, state))
            })
            .collect())
    }

    /// Run one step's action, retrying retryable failures and per-attempt
    /// timeouts within its retry budget.
    ///
    /// A step `timeout_seconds` earlier than the execution deadline bounds
    /// each attempt; otherwise the execution deadline governs and its expiry
    /// surfaces as [`EngineError::DeadlineExceeded`] instead of a step
    /// failure.
    async fn attempt(&self, step: &Step, bindings: &Bindings, deadline: Instant) -> Attempted {
        let _permit = self.permits.acquire().await;
        let started = Instant::now();
        let mut attempts = 0;
        loop {
            attempts += 1;
            let timeout = step
                .timeout_seconds
                .map(|seconds| Instant::now() + Duration::from_secs(u64::from(seconds)))
                .filter(|at| *at < deadline);
            let call = BlockCall {
                id: &step.id,
                position: Position::Step,
                block: &step.action,
                inputs: bindings.inputs.clone(),
                workspace: bindings.workspace.clone(),
                deadline: timeout.unwrap_or(deadline),
            };
            let result = match timeout {
                Some(at) => timeout_at(at, self.actions.execute(call))
                    .await
                    .unwrap_or_else(|_| {
                        Err(BlockError::Retryable {
                            reason: "timed out".to_owned(),
                        })
                    }),
                None => self.actions.execute(call).await,
            };
            match result {
                Err(error) if error.is_retryable() && attempts <= step.retries => {
                    tracing::debug!(step = step.id.as_str(), attempts, %error, "retrying step");
                }
                result => {
                    return Attempted {
                        attempts,
                        duration: started.elapsed(),
                        result: result.map_err(|error| reason(&error)),
                    };
                }
            }
        }
    }

    /// Judge the requirements at `keys`, concurrently, returning results in
    /// key order.
    ///
    /// A requirement of a step without output, or whose bindings cannot be
    /// supplied, takes the gap's outcome without calling its check.
    ///
    /// # Errors
    /// Returns [`EngineError::Join`] when a check task panics and
    /// [`EngineError::Capture`] when an actual value cannot be hashed.
    async fn run_requirements(
        &self,
        spec: &Arc<VerifierSpec>,
        scope: &Scope<'_>,
        keys: impl Iterator<Item = (Option<usize>, usize)>,
        deadline: Instant,
    ) -> Result<Vec<RequirementResult>, EngineError> {
        let keys: Vec<_> = keys.collect();
        let mut judged = HashMap::new();
        let mut running = JoinSet::new();
        for &key in &keys {
            let requirement = requirement_at(spec, key);
            let blocked = key
                .0
                .and_then(|index| match scope.step(&spec.steps[index].id) {
                    Some(Settled::NoOutput { gap, .. }) => Some(gap.clone()),
                    _ => None,
                });
            match blocked.map_or_else(|| check_bindings(scope, requirement), Err) {
                Err(gap) => {
                    judged.insert(key, Judged::missing(gap));
                }
                Ok(inputs) => {
                    let engine = self.clone();
                    let spec = Arc::clone(spec);
                    running.spawn(
                        async move {
                            let requirement = requirement_at(&spec, key);
                            let step = key.0.map(|index| &spec.steps[index].id);
                            let judged = engine.judge(requirement, step, inputs, deadline).await;
                            (key, judged)
                        }
                        .instrument(tracing::Span::current()),
                    );
                }
            }
        }
        while let Some(joined) = running.join_next().await {
            let (key, done) = joined.map_err(|error| EngineError::Join(error.to_string()))?;
            judged.insert(key, done);
        }
        let capture = spec.context_capture.unwrap_or(EvalContextCapture::Full);
        keys.into_iter()
            .filter_map(|key| Some((key, judged.remove(&key)?)))
            .map(|(key, done)| {
                let requirement = requirement_at(spec, key);
                Ok(RequirementResult {
                    id: requirement.id.clone(),
                    step: key.0.map(|index| spec.steps[index].id.clone()),
                    statement: requirement.statement.clone(),
                    outcome: done.outcome,
                    reason: done.reason,
                    actual: capture_actual(done.actual, capture)?,
                    citations: done.citations,
                    detail: done.detail,
                    check: requirement.check.clone(),
                    provenance: done.provenance,
                })
            })
            .collect()
    }

    /// Run one requirement's check over its resolved bindings.
    async fn judge(
        &self,
        requirement: &Requirement,
        step: Option<&TaskId>,
        inputs: BTreeMap<String, CallInput>,
        deadline: Instant,
    ) -> Judged {
        let _permit = self.permits.acquire().await;
        match &requirement.check {
            Check::Assert {
                each,
                operator,
                expected,
                ..
            } => {
                let observed = inputs.get("value").map_or(Value::Null, json);
                let compare = if each.is_some() {
                    evaluate_item_mode
                } else {
                    evaluate_single
                };
                Judged::compared(compare(&requirement.id, operator, expected, &observed))
            }
            Check::Trace(trace) => self.trace(&requirement.id, trace, &inputs, deadline).await,
            Check::Agent(_) | Check::Code(_) => {
                let sources: BTreeSet<SlotName> = inputs
                    .values()
                    .flat_map(|input| input.sources.iter().cloned())
                    .collect();
                let call = BlockCall {
                    id: &requirement.id,
                    position: step
                        .map_or(Position::VerifierCheck, |step| Position::StepCheck { step }),
                    block: &requirement.check,
                    inputs,
                    workspace: BTreeMap::new(),
                    deadline,
                };
                match self.checks.execute(call).await {
                    Ok(output) => Judged::cited(output, &sources),
                    Err(error) => Judged::inconclusive(reason(&error)),
                }
            }
            Check::Drift(drift) => Judged::checked(self.drift.judge(&requirement.id, drift)),
        }
    }

    /// Compare the selected span value of the declared trace.
    ///
    /// A trace that is not supplied or cannot be read is inconclusive; a
    /// selector matching nothing compares `null`, as Eval trace tasks do.
    async fn trace(
        &self,
        id: &TaskId,
        trace: &TraceCheck,
        inputs: &BTreeMap<String, CallInput>,
        deadline: Instant,
    ) -> Judged {
        let Some(CallValue::Evidence(InputDeclaration::Trace { trace_id })) =
            inputs.get("on").map(|input| &input.value)
        else {
            return Judged::inconclusive(format!("`{}` supplied no trace", trace.on));
        };
        let wait = deadline.saturating_duration_since(Instant::now());
        let spans = match self.traces.fetch(*trace_id, wait).await {
            Ok(spans) => spans,
            Err(error) => return Judged::inconclusive(error.to_string()),
        };
        let document = project_spans(*trace_id, &spans);
        Judged::compared(
            extract_jsonpath_from(&document, &trace.span_selector).and_then(|observed| {
                evaluate_single(id, &trace.operator, &trace.expected, &observed)
            }),
        )
    }
}

/// A step's resolved action bindings.
struct Bindings {
    /// Values by authored input name; an extract source is `from`.
    inputs: BTreeMap<String, CallInput>,
    /// Workspace files by authored path.
    workspace: BTreeMap<String, CallInput>,
}

impl Bindings {
    /// The original slots every binding derives from.
    fn sources(&self) -> BTreeSet<SlotName> {
        self.inputs
            .values()
            .chain(self.workspace.values())
            .flat_map(|input| input.sources.iter().cloned())
            .collect()
    }
}

/// Resolve a step's `when` and action bindings and evaluate `when`.
///
/// # Errors
/// Returns the strongest gap among the `when` and action bindings; when every
/// value resolves, a false `when` is an intentional skip, an unevaluable
/// `when` or an extract source without evidence is a failure.
fn prepare(scope: &Scope<'_>, step: &Step) -> Result<Bindings, Missing> {
    let when = step
        .when
        .as_ref()
        .map(|when| scope.resolve(&when.path, None))
        .transpose();
    let empty = || Ok(BTreeMap::new());
    let (inputs, workspace) = match &step.action {
        Action::Extract { from } => (scope.resolve_all([("from", from)], None), empty()),
        Action::Agent(call) => (
            scope.resolve_all(&call.inputs, None),
            scope.resolve_all(&call.workspace, None),
        ),
        Action::Code(call) => (scope.resolve_all(&call.inputs, None), empty()),
    };
    let (when, inputs, workspace) = match (when, inputs, workspace) {
        (Ok(when), Ok(inputs), Ok(workspace)) => (when, inputs, workspace),
        (when, inputs, workspace) => {
            let gaps = [when.err(), inputs.err(), workspace.err()]
                .into_iter()
                .flatten();
            return Err(Missing::strongest(gaps)
                .unwrap_or_else(|| Missing::failed("a binding did not resolve")));
        }
    };
    if let Action::Extract { from } = &step.action
        && !matches!(
            inputs.get("from").map(|input| &input.value),
            Some(CallValue::Evidence(_))
        )
    {
        return Err(Missing::failed(format!("`{from}` supplied no evidence")));
    }
    if let (Some(condition), Some(observed)) = (&step.when, when) {
        let path = &condition.path;
        match operators::evaluate_operator(
            &json(&observed),
            &condition.operator,
            &condition.expected,
        ) {
            Ok(verdict) if verdict.passed => {}
            Ok(_) => {
                return Err(Missing {
                    cause: Cause::Skipped,
                    reason: format!("when `{path}` was false"),
                });
            }
            Err(error) => {
                return Err(Missing::failed(format!(
                    "when `{path}` cannot be evaluated: {error}"
                )));
            }
        }
    }
    Ok(Bindings { inputs, workspace })
}

/// Resolve the bindings a check reads; drift reads its executor's window.
///
/// # Errors
/// Returns the strongest gap among the bindings.
fn check_bindings(
    scope: &Scope<'_>,
    requirement: &Requirement,
) -> Result<BTreeMap<String, CallInput>, Missing> {
    let current = Some(requirement);
    match &requirement.check {
        Check::Assert { path, each, .. } => {
            scope.resolve_all(path.iter().chain(each).map(|path| ("value", path)), current)
        }
        Check::Trace(trace) => scope.resolve_all([("on", &trace.on)], current),
        Check::Agent(call) => scope.resolve_all(&call.inputs, current),
        Check::Code(call) => scope.resolve_all(&call.inputs, current),
        Check::Drift(_) => Ok(BTreeMap::new()),
    }
}

/// The result and settled state of a step that did not run.
fn skipped(step: &Step, gap: Missing) -> (StepResult, Settled) {
    let result = StepResult {
        id: step.id.clone(),
        status: StepStatus::Skipped,
        output: None,
        attempts: 0,
        duration_ms: 0,
        reason: Some(gap.reason.clone()),
        provenance: None,
        tool_calls: Vec::new(),
    };
    let settled = Settled::NoOutput {
        status: StepStatus::Skipped,
        gap,
    };
    (result, settled)
}

/// The final attempt of one step action.
struct Attempted {
    /// Attempts made, retries included.
    attempts: u32,
    /// Wall time across every attempt.
    duration: Duration,
    /// The output, or the final failure reason.
    result: Result<ActionOutput, String>,
}

impl Attempted {
    /// The step's result and the state later blocks read.
    fn settle(self, step: &Step, sources: BTreeSet<SlotName>) -> (StepResult, Settled) {
        let duration_ms = u64::try_from(self.duration.as_millis()).unwrap_or(u64::MAX);
        match self.result {
            Ok(output) => (
                StepResult {
                    id: step.id.clone(),
                    status: StepStatus::Succeeded,
                    output: Some(output.output.clone()),
                    attempts: self.attempts,
                    duration_ms,
                    reason: None,
                    provenance: output.provenance,
                    tool_calls: output.tool_calls,
                },
                Settled::Succeeded {
                    output: output.output,
                    sources,
                },
            ),
            Err(reason) => (
                StepResult {
                    id: step.id.clone(),
                    status: StepStatus::Failed,
                    output: None,
                    attempts: self.attempts,
                    duration_ms,
                    reason: Some(reason.clone()),
                    provenance: None,
                    tool_calls: Vec::new(),
                },
                Settled::NoOutput {
                    status: StepStatus::Failed,
                    gap: Missing::failed(reason),
                },
            ),
        }
    }
}

/// One requirement's judgment before capture and attribution.
struct Judged {
    /// The outcome.
    outcome: RequirementOutcome,
    /// Why it holds.
    reason: String,
    /// The compared actual value.
    actual: Option<Value>,
    /// Citations of reachable original evidence.
    citations: Vec<Citation>,
    /// Check-specific detail.
    detail: Option<RequirementDetail>,
    /// What executed the check.
    provenance: Option<CallProvenance>,
}

impl Judged {
    /// The outcome of evidence that could not be supplied.
    fn missing(gap: Missing) -> Self {
        Self::bare(gap.cause.outcome(), gap.reason)
    }

    /// An inconclusive judgment, such as an isolated check failure.
    fn inconclusive(reason: String) -> Self {
        Self::bare(RequirementOutcome::Inconclusive, reason)
    }

    /// A judgment with only an outcome and reason.
    fn bare(outcome: RequirementOutcome, reason: String) -> Self {
        Self {
            outcome,
            reason,
            actual: None,
            citations: Vec::new(),
            detail: None,
            provenance: None,
        }
    }

    /// A deterministic comparison: passed or failed, or inconclusive when the
    /// values cannot be compared.
    fn compared(compared: Result<Compared, EvalExecError>) -> Self {
        match compared {
            Ok((passed, actual, _, message)) => {
                let outcome = if passed {
                    RequirementOutcome::Passed
                } else {
                    RequirementOutcome::Failed
                };
                Self {
                    actual,
                    ..Self::bare(
                        outcome,
                        message.unwrap_or_else(|| "the comparison held".to_owned()),
                    )
                }
            }
            Err(error) => Self::inconclusive(error.to_string()),
        }
    }

    /// An executor's judgment, unchanged.
    fn checked(output: CheckOutput) -> Self {
        let outcome = match output.verdict.outcome {
            VerificationVerdict::Passed => RequirementOutcome::Passed,
            VerificationVerdict::Failed => RequirementOutcome::Failed,
            VerificationVerdict::Inconclusive => RequirementOutcome::Inconclusive,
        };
        Self {
            outcome,
            reason: output.verdict.reason,
            actual: output.actual,
            citations: output.verdict.citations,
            detail: output.detail,
            provenance: output.provenance,
        }
    }

    /// An Agent or code judgment: citations outside the call's original
    /// `sources` are dropped, and a pass left without a citation is
    /// inconclusive.
    fn cited(output: CheckOutput, sources: &BTreeSet<SlotName>) -> Self {
        let mut judged = Self::checked(output);
        judged
            .citations
            .retain(|citation| sources.contains(&citation.slot));
        if judged.outcome == RequirementOutcome::Passed && judged.citations.is_empty() {
            judged.outcome = RequirementOutcome::Inconclusive;
            judged.reason = format!(
                "passed without a citation of reachable original evidence: {}",
                judged.reason
            );
        }
        judged
    }
}

/// The requirement at `key`: a step's requirement, or a whole-Verifier one.
fn requirement_at(spec: &VerifierSpec, (step, at): (Option<usize>, usize)) -> &Requirement {
    match step {
        Some(index) => &spec.steps[index].requirements[at],
        None => &spec.requirements[at],
    }
}

/// The JSON a comparison reads: evidence compares as its declaration.
fn json(input: &CallInput) -> Value {
    match &input.value {
        CallValue::Json(value) => value.clone(),
        CallValue::Evidence(declaration) => {
            serde_json::to_value(declaration).unwrap_or(Value::Null)
        }
    }
}

/// The bounded cause a block error carries.
fn reason(error: &BlockError) -> String {
    match error {
        BlockError::Retryable { reason } | BlockError::Terminal { reason } => reason.clone(),
    }
}

/// Apply `rule` to the requirement outcomes.
///
/// Not-applicable outcomes leave the denominator; none applicable is
/// inconclusive. `all` fails on any failure, else is inconclusive on any
/// inconclusive. `pass_rate` passes when passed / applicable reaches the
/// threshold, counting inconclusive outcomes as applicable; below it, any
/// inconclusive outcome makes it inconclusive, otherwise it fails.
fn decide(rule: Decide, outcomes: impl Iterator<Item = RequirementOutcome>) -> VerificationVerdict {
    let (mut passed, mut failed, mut inconclusive) = (0_u32, 0_u32, 0_u32);
    for outcome in outcomes {
        match outcome {
            RequirementOutcome::Passed => passed += 1,
            RequirementOutcome::Failed => failed += 1,
            RequirementOutcome::Inconclusive => inconclusive += 1,
            RequirementOutcome::NotApplicable => {}
        }
    }
    let applicable = passed + failed + inconclusive;
    if applicable == 0 {
        return VerificationVerdict::Inconclusive;
    }
    let reached = match rule {
        Decide::All => failed == 0 && inconclusive == 0,
        Decide::PassRate { threshold } => f64::from(passed) / f64::from(applicable) >= threshold,
    };
    match rule {
        Decide::All if failed > 0 => VerificationVerdict::Failed,
        _ if reached => VerificationVerdict::Passed,
        _ if inconclusive > 0 => VerificationVerdict::Inconclusive,
        _ => VerificationVerdict::Failed,
    }
}

#[cfg(test)]
mod tests;
