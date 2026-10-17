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
use tokio::time::{Instant, timeout_at};

use wyrd_spec::card::verifier::{
    Action, Check, Decide, InputSlot, Requirement, Step, TraceCheck, VerifierSpec,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::ContextCapture;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_spec::verification::execution::{
    CallProvenance, Citation, InputDeclaration, RequirementDetail, RequirementOutcome,
    RequirementResult, StepResult, StepStatus, VerificationOutcome,
};
use wyrd_spec::verification::{ExecutionPlan, Stage, TaskId};

use super::drift::DriftChecks;
use super::scope::{Cause, Done, Missing, Scope, Settled};
use super::{
    ActionExecutor, ActionOutput, BlockCall, BlockError, CallInput, CallValue, CheckExecutor,
    CheckOutput, EvidenceReader, Position,
};
use crate::assertion::capture_actual;
use crate::assertion::{Compared, evaluate_item_mode, evaluate_single};
use crate::error::ExecError;
use crate::jsonpath::extract_jsonpath_from;
use crate::operators;
use crate::trace_source::TraceSource;
use crate::trace_source::project_spans;
use crate::walk::{Prepared, RunLedger, walk};

/// Requirement reason of an `each` assertion whose selector matched nothing.
const EMPTY_SELECTION: &str = "empty_selection";

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
    Capture(#[from] ExecError),
    /// A block task panicked or was aborted.
    #[error("a verification block task stopped unexpectedly: {0}")]
    Join(String),
    /// A block was refused process capacity at admission
    /// ([`BlockError::Busy`]); unfinished work was cancelled and no verdict
    /// exists, so the host may run the whole execution again later.
    #[error("a verification block was refused process capacity: {0}")]
    Busy(String),
    /// The supplied evidence does not conform to the Verifier's declared
    /// input slots: an undeclared slot, a declaration of another kind, a
    /// JSON value violating its slot schema, a file in a format its media
    /// slot does not accept, or evidence the host refuses to admit. No
    /// block ran.
    #[error("the verification input is invalid: {0}")]
    InputInvalid(String),
    /// The supplied evidence exceeds a media slot's `max_bytes` or the
    /// host's combined input-file bound. No block ran.
    #[error("the verification input is too large: {0}")]
    InputTooLarge(String),
    /// The host cannot read the evidence metadata admission needs now. No
    /// block ran, so the whole execution may run again later.
    #[error("the verification evidence cannot be admitted now: {0}")]
    EvidenceUnavailable(String),
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
    /// The host's evidence reader, asked to admit the pinned evidence
    /// before any block runs; `None` admits what slot validation accepts.
    evidence: Option<Arc<dyn EvidenceReader>>,
}

impl VerificationEngine {
    /// An engine running at most `concurrency` blocks at once, with no
    /// scheduled drift evidence.
    ///
    /// # Panics
    /// Panics when `concurrency` exceeds [`Semaphore::MAX_PERMITS`].
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
            evidence: None,
        }
    }

    /// Admit each execution's pinned evidence through `evidence` before any
    /// block runs, after the engine's own slot validation.
    #[must_use]
    pub fn with_evidence(mut self, evidence: Arc<dyn EvidenceReader>) -> Self {
        self.evidence = Some(evidence);
        self
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
    /// The deadline is checked before any work and again before the verdict
    /// is published, so work that completes without suspending, such as an
    /// empty Verifier, never reports a verdict outside its budget; pending
    /// work is cancelled when the deadline passes.
    ///
    /// Before any block runs, every supplied declaration must conform to its
    /// declared slot (see [`conform`]) and the host's evidence reader, when
    /// set, must admit the whole set. A declared slot left unsupplied is not
    /// refused here: a required one leaves its dependents without evidence
    /// and an optional one reads as null.
    ///
    /// # Errors
    /// Returns [`EngineError::InputInvalid`] or [`EngineError::InputTooLarge`]
    /// for nonconforming evidence and [`EngineError::EvidenceUnavailable`]
    /// when the host cannot admit it now, [`EngineError::Plan`] for an
    /// unplannable graph,
    /// [`EngineError::DeadlineExceeded`] when `deadline` is reached before
    /// the verdict, [`EngineError::Capture`] when an actual value cannot be
    /// hashed, [`EngineError::Join`] when a block task panics, and
    /// [`EngineError::Busy`] when any action or check is refused process
    /// capacity.
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
        let expired = || Instant::now() >= deadline;
        if expired() {
            return Err(EngineError::DeadlineExceeded);
        }
        conform(&spec, &evidence)?;
        let plan = spec.plan().map_err(EngineError::Plan)?;
        let ledger = timeout_at(deadline, async {
            if let Some(reader) = &self.evidence {
                reader.admit(&spec.inputs, &evidence).await?;
            }
            self.drive(&spec, &plan, &evidence, deadline).await
        })
        .await
        .map_err(|_| EngineError::DeadlineExceeded)??;
        let (mut steps, mut requirements) = (Vec::new(), Vec::new());
        for (_, done) in ledger {
            match done {
                Done::Step { result, .. } => steps.push(*result),
                Done::Requirement(result) => requirements.push(*result),
            }
        }
        let verdict = decide(
            spec.decide,
            requirements.iter().map(|result| result.outcome),
        );
        if expired() {
            return Err(EngineError::DeadlineExceeded);
        }
        Ok(VerificationOutcome {
            verdict,
            requirements,
            steps,
            evidence,
        })
    }

    /// Walk every plan stage's steps, then that stage's step requirements,
    /// then the whole-Verifier requirements, through the shared stage walk.
    ///
    /// `plan` must be `spec.plan()`, so every planned id names a declared
    /// step; requirement ids are globally unique, so steps and requirements
    /// share one ledger.
    ///
    /// # Errors
    /// Returns [`EngineError::Capture`], [`EngineError::Join`], or
    /// [`EngineError::Busy`].
    ///
    /// # Panics
    /// Panics when `plan` names a step `spec` does not declare.
    async fn drive(
        &self,
        spec: &Arc<VerifierSpec>,
        plan: &ExecutionPlan,
        evidence: &BTreeMap<SlotName, InputDeclaration>,
        deadline: Instant,
    ) -> Result<RunLedger<Done>, EngineError> {
        let mut blocks: HashMap<&TaskId, Block> = HashMap::new();
        for (index, step) in spec.steps.iter().enumerate() {
            blocks.insert(&step.id, Block::Step(index));
            for (at, requirement) in step.requirements.iter().enumerate() {
                blocks.insert(&requirement.id, Block::Requirement((Some(index), at)));
            }
        }
        for (at, requirement) in spec.requirements.iter().enumerate() {
            blocks.insert(&requirement.id, Block::Requirement((None, at)));
        }
        let mut stages = Vec::with_capacity(plan.stages.len() * 2 + 1);
        for stage in &plan.stages {
            stages.push(stage.clone());
            stages.push(requirement_stage(stage.tasks.iter().flat_map(
                |id| match blocks.get(id) {
                    Some(Block::Step(index)) => spec.steps[*index].requirements.as_slice(),
                    _ => panic!("the plan names only declared steps"),
                },
            )));
        }
        stages.push(requirement_stage(spec.requirements.iter()));
        walk(
            &stages,
            |ledger: &RunLedger<Done>, stage| {
                let scope = Scope::new(spec, evidence, ledger);
                stage
                    .tasks
                    .iter()
                    .map(|id| match blocks.get(id) {
                        Some(Block::Step(index)) => Ok(self.step(spec, &scope, *index, deadline)),
                        Some(Block::Requirement(key)) => {
                            self.requirement(spec, &scope, *key, deadline)
                        }
                        None => panic!("the plan names only declared steps"),
                    })
                    .collect()
            },
            |error| EngineError::Join(error.to_string()),
        )
        .await
    }

    /// Prepare one step: a step whose bindings or `when` leave a gap is
    /// skipped with that gap, otherwise its action runs under the
    /// concurrency bound. A running action refused process capacity fails the
    /// stage with [`EngineError::Busy`].
    ///
    /// # Panics
    /// Panics when `index` is not a declared step of `spec`.
    fn step(
        &self,
        spec: &Arc<VerifierSpec>,
        scope: &Scope<'_>,
        index: usize,
        deadline: Instant,
    ) -> Prepared<Done, EngineError> {
        let step = &spec.steps[index];
        match prepare(scope, step) {
            Err(gap) => Prepared::Settled(skipped(step, gap)),
            Ok(bindings) => {
                let engine = self.clone();
                let spec = Arc::clone(spec);
                Prepared::Run(Box::pin(async move {
                    let step = &spec.steps[index];
                    let attempted = engine.attempt(step, &bindings, deadline).await?;
                    Ok(attempted.settle(step, bindings.sources()))
                }))
            }
        }
    }

    /// Prepare one requirement at `key`.
    ///
    /// Its evidence gaps are the containing step's missing output, if any,
    /// and every explicit binding gap; the strongest cause wins, so a failed
    /// binding is never hidden behind an intentionally skipped owner. A
    /// requirement with a gap takes the gap's outcome without calling its
    /// check; otherwise the check runs under the concurrency bound.
    ///
    /// # Errors
    /// Returns [`EngineError::Capture`] when an actual value cannot be
    /// hashed; the prepared check fails with [`EngineError::Busy`] when it is
    /// refused process capacity.
    ///
    /// # Panics
    /// Panics when `key` does not name a requirement of `spec`.
    fn requirement(
        &self,
        spec: &Arc<VerifierSpec>,
        scope: &Scope<'_>,
        key: Key,
        deadline: Instant,
    ) -> Result<Prepared<Done, EngineError>, EngineError> {
        let requirement = requirement_at(spec, key);
        let owner = key
            .0
            .and_then(|index| match scope.step(&spec.steps[index].id) {
                Some(Settled::NoOutput { gap, .. }) => Some(gap.clone()),
                _ => None,
            });
        let (gap, inputs) = match check_bindings(scope, requirement) {
            Ok(inputs) => (owner, Some(inputs)),
            Err(gap) => (Missing::strongest(owner.into_iter().chain([gap])), None),
        };
        match (gap, inputs) {
            (None, Some(inputs)) => {
                let engine = self.clone();
                let spec = Arc::clone(spec);
                Ok(Prepared::Run(Box::pin(async move {
                    let requirement = requirement_at(&spec, key);
                    let step = key.0.map(|index| &spec.steps[index].id);
                    let judged = engine.judge(requirement, step, inputs, deadline).await?;
                    judged.report(&spec, key)
                })))
            }
            (gap, _) => {
                Judged::missing(gap.unwrap_or_else(|| Missing::failed("a binding did not resolve")))
                    .report(spec, key)
                    .map(Prepared::Settled)
            }
        }
    }

    /// Run one step's action, retrying retryable failures and per-attempt
    /// timeouts within its retry budget.
    ///
    /// A step `timeout_seconds` earlier than the execution deadline bounds
    /// each attempt; otherwise the execution deadline governs and its expiry
    /// surfaces as [`EngineError::DeadlineExceeded`] instead of a step
    /// failure.
    ///
    /// # Errors
    /// Returns [`EngineError::Busy`] as soon as an attempt is refused process
    /// capacity, without spending the step's retries or settling the step.
    async fn attempt(
        &self,
        step: &Step,
        bindings: &Bindings,
        deadline: Instant,
    ) -> Result<Attempted, EngineError> {
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
                Err(BlockError::Busy { reason }) => return Err(EngineError::Busy(reason)),
                Err(error) if error.is_retryable() && attempts <= step.retries => {
                    tracing::debug!(step = step.id.as_str(), attempts, %error, "retrying step");
                }
                result => {
                    return Ok(Attempted {
                        attempts,
                        duration: started.elapsed(),
                        result: result.map_err(|error| reason(&error)),
                    });
                }
            }
        }
    }

    /// Run one requirement's check over its resolved bindings.
    ///
    /// A failed Agent or code check is inconclusive, and so is an `each`
    /// assertion selecting no value, with reason [`EMPTY_SELECTION`], so an
    /// empty selection never passes vacuously.
    ///
    /// # Errors
    /// Returns [`EngineError::Busy`] when an Agent or code check is refused
    /// process capacity, which is never judged.
    async fn judge(
        &self,
        requirement: &Requirement,
        step: Option<&TaskId>,
        inputs: BTreeMap<String, CallInput>,
        deadline: Instant,
    ) -> Result<Judged, EngineError> {
        let _permit = self.permits.acquire().await;
        Ok(match &requirement.check {
            Check::Assert {
                each,
                operator,
                expected,
                ..
            } => {
                let observed = inputs.get("value").map_or(Value::Null, json);
                match (each, &observed) {
                    (Some(_), Value::Array(items)) if items.is_empty() => {
                        Judged::inconclusive(EMPTY_SELECTION.to_owned())
                    }
                    (Some(_), _) => Judged::compared(evaluate_item_mode(
                        &requirement.id,
                        operator,
                        expected,
                        &observed,
                    )),
                    (None, _) => Judged::compared(evaluate_single(
                        &requirement.id,
                        operator,
                        expected,
                        &observed,
                    )),
                }
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
                    Err(BlockError::Busy { reason }) => return Err(EngineError::Busy(reason)),
                    Err(error) => Judged::inconclusive(reason(&error)),
                }
            }
            Check::Drift(drift) => Judged::checked(self.drift.judge(&requirement.id, drift)),
        })
    }

    /// Compare the selected span value of the declared trace.
    ///
    /// A trace that is not supplied or cannot be read is inconclusive; a
    /// selector matching nothing compares `null`.
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

/// Where a Verifier block is declared.
#[derive(Clone, Copy)]
enum Block {
    /// The step at this index of `steps`.
    Step(usize),
    /// The requirement at this key.
    Requirement(Key),
}

/// A requirement's declaration: the index of its containing step, or `None`
/// for a whole-Verifier requirement, and its index in that list.
type Key = (Option<usize>, usize);

/// The stage judging `requirements`, in their declaration order.
///
/// Its index is unused: Verifier blocks never read their stage index.
fn requirement_stage<'r>(requirements: impl Iterator<Item = &'r Requirement>) -> Stage {
    Stage {
        index: 0,
        tasks: requirements
            .map(|requirement| requirement.id.clone())
            .collect(),
    }
}

/// The result and settled state of a step that did not run.
fn skipped(step: &Step, gap: Missing) -> Done {
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
    let state = Settled::NoOutput {
        status: StepStatus::Skipped,
        gap,
    };
    Done::Step {
        result: Box::new(result),
        state,
    }
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
    fn settle(self, step: &Step, sources: BTreeSet<SlotName>) -> Done {
        let duration_ms = u64::try_from(self.duration.as_millis()).unwrap_or(u64::MAX);
        let (result, state) = match self.result {
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
        };
        Done::Step {
            result: Box::new(result),
            state,
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
    /// The reported result of the requirement at `key`, its actual value
    /// captured under the Verifier's context-capture mode.
    ///
    /// # Errors
    /// Returns [`EngineError::Capture`] when the actual value cannot be
    /// hashed.
    ///
    /// # Panics
    /// Panics when `key` does not name a requirement of `spec`.
    fn report(self, spec: &VerifierSpec, key: Key) -> Result<Done, EngineError> {
        let requirement = requirement_at(spec, key);
        let capture = spec.context_capture.unwrap_or(ContextCapture::Full);
        Ok(Done::Requirement(Box::new(RequirementResult {
            id: requirement.id.clone(),
            step: key.0.map(|index| spec.steps[index].id.clone()),
            statement: requirement.statement.clone(),
            outcome: self.outcome,
            reason: self.reason,
            actual: capture_actual(self.actual, capture)?,
            citations: self.citations,
            detail: self.detail,
            check: requirement.check.clone(),
            provenance: self.provenance,
        })))
    }

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
    fn compared(compared: Result<Compared, ExecError>) -> Self {
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
///
/// # Panics
/// Panics when `key` does not index a declared requirement; keys come only
/// from `spec`'s own declarations.
fn requirement_at(spec: &VerifierSpec, (step, at): Key) -> &Requirement {
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
        BlockError::Retryable { reason }
        | BlockError::Terminal { reason }
        | BlockError::Busy { reason } => reason.clone(),
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

/// Refuse supplied `evidence` that does not conform to `spec`'s declared
/// input slots.
///
/// Every supplied slot must be declared, and its declaration must be the
/// slot's kind: inline JSON for a `json` slot, a file or Artifact for a
/// `media` slot, and a trace for a `trace` slot; `samples` slots are filled
/// only by a schedule. A JSON value must satisfy its slot schema, and a file
/// must be in one of its media slot's formats and within its `max_bytes`.
/// An Artifact's format and size are known only to the host, which checks
/// them when it admits the evidence. Missing slots are not refused here.
///
/// # Errors
/// Returns [`EngineError::InputTooLarge`] for a file above its slot's
/// `max_bytes` and [`EngineError::InputInvalid`] for every other
/// nonconformity, naming the slot.
fn conform(
    spec: &VerifierSpec,
    evidence: &BTreeMap<SlotName, InputDeclaration>,
) -> Result<(), EngineError> {
    for (name, declaration) in evidence {
        let invalid = |reason: &str| EngineError::InputInvalid(format!("input {name}: {reason}"));
        let slot = spec
            .inputs
            .get(name)
            .ok_or_else(|| invalid("the Verifier declares no such input"))?;
        match (slot, declaration) {
            (InputSlot::Json { schema, .. }, InputDeclaration::Json { value }) => {
                if let Some(schema) = schema {
                    let validator = jsonschema::validator_for(schema).map_err(|error| {
                        invalid(&format!("the slot schema is invalid: {error}"))
                    })?;
                    if let Some(error) = validator.iter_errors(value).next() {
                        return Err(invalid(&format!(
                            "the value violates the slot schema at {}: {error}",
                            error.instance_path()
                        )));
                    }
                }
            }
            (
                InputSlot::Media {
                    formats, max_bytes, ..
                },
                InputDeclaration::File {
                    format, size_bytes, ..
                },
            ) => {
                if !formats.contains(format) {
                    return Err(invalid(&format!("the slot does not accept {format}")));
                }
                if let Some(max) = max_bytes.filter(|max| size_bytes > max) {
                    return Err(EngineError::InputTooLarge(format!(
                        "input {name}: the file exceeds the slot's {max}-byte limit"
                    )));
                }
            }
            (InputSlot::Media { .. }, InputDeclaration::Artifact { .. })
            | (InputSlot::Trace { .. }, InputDeclaration::Trace { .. }) => {}
            _ => return Err(invalid("the declaration is not the slot's kind")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
