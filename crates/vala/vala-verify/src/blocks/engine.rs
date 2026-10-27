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

use serde_json::{Map, Value};
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::time::{Instant, timeout_at};

use wyrd_spec::card::verifier::{
    Action, AgentCall, ArtifactSelection, Check, Decide, InputSlot, Requirement, Step, TraceCheck,
    VerifierSpec, WorkspaceSource,
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
use crate::trace_source::project_spans;
use crate::trace_source::{TraceSource, TraceUnavailable};
use crate::walk::{Prepared, RunLedger, walk};
use wyrd_spec::vala::ids::TraceId;
use wyrd_spec::vala::trace::SpanRecord;

/// Requirement reason of an `each` assertion whose selector matched nothing.
const EMPTY_SELECTION: &str = "empty_selection";

/// Histogram of every action attempt and check the engine runs, seconds,
/// labelled `block` (`action` or `check`), `kind` (the block kind), and
/// `outcome` (`ok`, or `error` when the block failed rather than judged).
/// It times the block itself, after its concurrency permit; the host
/// registers its buckets.
pub const BLOCK_DURATION_SECONDS: &str = "wyrd_verification_block_duration_seconds";

/// The intervals one execution's actions and checks ran, shared by every
/// block of that execution.
///
/// This is the Verifier's own execution time: the host subtracts its union
/// from the whole execution to get the runtime's overhead. The engine
/// records into it; the host reads it once the execution settles. Cloning
/// shares the same record.
#[derive(Debug, Clone, Default)]
pub struct BlockClock {
    /// `[start, end)` of every finished block, in completion order.
    intervals: Arc<std::sync::Mutex<Vec<(std::time::Instant, std::time::Instant)>>>,
}

impl BlockClock {
    /// Every finished block's `[start, end)`, in completion order; blocks
    /// may overlap when they ran concurrently.
    #[must_use]
    pub fn intervals(&self) -> Vec<(std::time::Instant, std::time::Instant)> {
        self.lock().clone()
    }

    /// Record one finished block.
    fn record(&self, start: std::time::Instant, end: std::time::Instant) {
        self.lock().push((start, end));
    }

    /// Lock the record, recovering it from a poisoned lock: no code path
    /// panics while holding it, and timing must never fail an execution.
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(std::time::Instant, std::time::Instant)>> {
        self.intervals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Times one action attempt or check into [`BLOCK_DURATION_SECONDS`] and the
/// execution's [`BlockClock`].
///
/// Every label is a static string, so recording allocates nothing; a timer
/// dropped without [`BlockTimer::finish`], by cancellation, records nothing.
struct BlockTimer<'a> {
    /// The execution's record of block time.
    clock: &'a BlockClock,
    /// `action` or `check`.
    block: &'static str,
    /// The block kind, such as `assert` or `agent`.
    kind: &'static str,
    /// When the block started.
    started: std::time::Instant,
}

impl<'a> BlockTimer<'a> {
    /// Start timing one `block` of `kind` into `clock`.
    fn start(clock: &'a BlockClock, block: &'static str, kind: &'static str) -> Self {
        Self {
            clock,
            block,
            kind,
            started: std::time::Instant::now(),
        }
    }

    /// Record the block's interval on the clock and its elapsed time under
    /// `ok` (`outcome="ok"`) or a failure (`outcome="error"`).
    fn finish(self, ok: bool) {
        let ended = std::time::Instant::now();
        self.clock.record(self.started, ended);
        metrics::histogram!(
            BLOCK_DURATION_SECONDS,
            "block" => self.block,
            "kind" => self.kind,
            "outcome" => if ok { "ok" } else { "error" }
        )
        .record((ended - self.started).as_secs_f64());
    }
}

/// The `kind` label of an action.
const fn action_kind(action: &Action) -> &'static str {
    match action {
        Action::Extract { .. } => "extract",
        Action::Agent(_) => "agent",
        Action::Code(_) => "code",
    }
}

/// The `kind` label of a check.
const fn check_kind(check: &Check) -> &'static str {
    match check {
        Check::Assert { .. } => "assert",
        Check::Trace(_) => "trace",
        Check::Agent(_) => "agent",
        Check::Code(_) => "code",
        Check::Drift(_) => "drift",
    }
}

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
    /// The host cannot read the evidence metadata admission needs now, or,
    /// on an engine built [`VerificationEngine::with_unlanded_traces_deferred`],
    /// a declared trace has not landed yet. No verdict exists, so the whole
    /// execution may run again later.
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
    /// Where every block's run interval is recorded.
    clock: BlockClock,
    /// Whether a not-yet-landed trace stops the run with
    /// [`EngineError::EvidenceUnavailable`] instead of an inconclusive check.
    defer_unlanded: bool,
    /// The current run's trace reads by trace ID; [`Self::run`] starts each
    /// run with none, so every check naming one trace in that run judges the
    /// same read and a later run reads afresh.
    reads: TraceReads,
    /// The current run's fixed workspace attachments; [`Self::run`] selects
    /// them afresh each run.
    attached: Arc<Attached>,
}

/// Each fixed Artifact selection of a run with its selected files: the path
/// relative to the selection, the synthetic evidence slot, and its pinned
/// declaration.
type Attached = Vec<(ArtifactSelection, Vec<(String, SlotName, InputDeclaration)>)>;

/// One run's read of each trace it names, shared by every check of that run.
type TraceReads = Arc<
    std::sync::Mutex<
        HashMap<
            TraceId,
            Arc<tokio::sync::OnceCell<Result<Arc<Vec<SpanRecord>>, TraceUnavailable>>>,
        >,
    >,
>;

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
            clock: BlockClock::default(),
            defer_unlanded: false,
            reads: TraceReads::default(),
            attached: Arc::default(),
        }
    }

    /// Stop a run whose declared trace has not landed yet with
    /// [`EngineError::EvidenceUnavailable`], so a queued host can retry the
    /// same execution once the evidence lands. Without it, such a trace check
    /// is inconclusive. Every other trace failure stays an inconclusive check.
    #[must_use]
    pub const fn with_unlanded_traces_deferred(mut self) -> Self {
        self.defer_unlanded = true;
        self
    }

    /// Record every block's run interval on `clock`, the host's measure of
    /// the Verifier's own execution time.
    #[must_use]
    pub fn with_clock(mut self, clock: BlockClock) -> Self {
        self.clock = clock;
        self
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
    /// when the host cannot admit it now or, on a deferring engine, a trace
    /// has not landed yet, [`EngineError::Plan`] for an
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
        let (ledger, evidence) = timeout_at(deadline, async {
            let (evidence, attached) = self.attach(&spec, evidence).await?;
            if let Some(reader) = &self.evidence {
                reader.admit(&spec.inputs, &evidence).await?;
            }
            let attempt = Self {
                reads: TraceReads::default(),
                attached: Arc::new(attached),
                ..self.clone()
            };
            let ledger = attempt.drive(&spec, &plan, &evidence, deadline).await?;
            Ok::<_, EngineError>((ledger, evidence))
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

    /// Select every fixed Artifact attachment's files through the evidence
    /// reader and pin each file as one undeclared evidence slot, so it is
    /// admitted, counted, and citable like any original input.
    ///
    /// A file selected twice, by repeated or overlapping selections, keeps
    /// one slot. Synthetic slot names never reuse a declared name.
    ///
    /// # Errors
    /// Returns [`EngineError::InputInvalid`] for an unpinned Artifact
    /// reference, a host without an evidence reader, or a selection the host
    /// refuses, and [`EngineError::EvidenceUnavailable`] when the host cannot
    /// select now.
    async fn attach(
        &self,
        spec: &VerifierSpec,
        mut evidence: BTreeMap<SlotName, InputDeclaration>,
    ) -> Result<(BTreeMap<SlotName, InputDeclaration>, Attached), EngineError> {
        let mut attached: Attached = Vec::new();
        let mut next = 0_usize;
        for selection in fixed_selections(spec) {
            if attached.iter().any(|(seen, _)| seen == selection) {
                continue;
            }
            let card = selection.card.as_card_ref().ok_or_else(|| {
                EngineError::InputInvalid(
                    "a workspace Artifact is not pinned to an exact Card".to_owned(),
                )
            })?;
            let reader = self.evidence.as_ref().ok_or_else(|| {
                EngineError::InputInvalid("this host mounts no Artifact files".to_owned())
            })?;
            let files = reader
                .select(card, selection.path.as_deref())
                .await
                .map_err(|error| {
                    let reason = format!("workspace Artifact {}: {error}", card.name);
                    match error {
                        BlockError::Terminal { .. } => EngineError::InputInvalid(reason),
                        _ => EngineError::EvidenceUnavailable(reason),
                    }
                })?;
            let mut mounted = Vec::with_capacity(files.len());
            for file in files {
                let relative = match selection.path.as_deref() {
                    None => file.clone(),
                    Some(path) if path == file => String::new(),
                    Some(path) => file
                        .strip_prefix(path)
                        .and_then(|rest| rest.strip_prefix('/'))
                        .ok_or_else(|| {
                            EngineError::InputInvalid(format!(
                                "workspace Artifact {}: {file} is outside the selected {path}",
                                card.name
                            ))
                        })?
                        .to_owned(),
                };
                let declaration = InputDeclaration::Artifact {
                    artifact: card.clone(),
                    path: Some(file),
                };
                let existing = evidence
                    .iter()
                    .find(|(name, pinned)| {
                        !spec.inputs.contains_key(*name) && **pinned == declaration
                    })
                    .map(|(name, _)| name.clone());
                let slot = if let Some(slot) = existing {
                    slot
                } else {
                    let slot = loop {
                        let candidate = SlotName::new(format!("attachment_{next}"))
                            .map_err(|error| EngineError::InputInvalid(error.to_string()))?;
                        next += 1;
                        if !evidence.contains_key(&candidate)
                            && !spec.inputs.contains_key(&candidate)
                        {
                            break candidate;
                        }
                    };
                    evidence.insert(slot.clone(), declaration.clone());
                    slot
                };
                mounted.push((relative, slot, declaration));
            }
            attached.push((selection.clone(), mounted));
        }
        Ok((evidence, attached))
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
        match prepare(scope, &self.attached, step) {
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
        let (gap, inputs) = match check_bindings(scope, &self.attached, requirement) {
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
    /// Each attempt is timed into [`BLOCK_DURATION_SECONDS`] after the
    /// permit is held, inside a debug `verification.block` span.
    ///
    /// # Errors
    /// Returns [`EngineError::Busy`] as soon as an attempt is refused process
    /// capacity, without spending the step's retries or settling the step.
    #[tracing::instrument(
        name = "verification.block",
        level = "debug",
        skip_all,
        fields(block = "action", kind = action_kind(&step.action), id = step.id.as_str())
    )]
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
            let timer = BlockTimer::start(&self.clock, "action", action_kind(&step.action));
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
            timer.finish(result.is_ok());
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
    /// The check is timed into [`BLOCK_DURATION_SECONDS`] after the permit
    /// is held, inside a debug `verification.block` span; a check that could
    /// not judge (an uncomparable value, an unreadable trace, a failed or
    /// refused executor) records `outcome="error"`.
    ///
    /// # Errors
    /// Returns [`EngineError::Busy`] when an Agent or code check is refused
    /// process capacity, which is never judged, and
    /// [`EngineError::EvidenceUnavailable`] when a deferring engine's trace
    /// has not landed yet.
    #[tracing::instrument(
        name = "verification.block",
        level = "debug",
        skip_all,
        fields(block = "check", kind = check_kind(&requirement.check), id = requirement.id.as_str())
    )]
    async fn judge(
        &self,
        requirement: &Requirement,
        step: Option<&TaskId>,
        bindings: Bindings,
        deadline: Instant,
    ) -> Result<Judged, EngineError> {
        let _permit = self.permits.acquire().await;
        let sources = bindings.sources();
        let Bindings { inputs, workspace } = bindings;
        let timer = BlockTimer::start(&self.clock, "check", check_kind(&requirement.check));
        let (judged, ok) = match &requirement.check {
            Check::Assert {
                each,
                operator,
                expected,
                ..
            } => {
                let observed = inputs.get("value").map_or(Value::Null, json);
                let compared = match (each, &observed) {
                    (Some(_), Value::Array(items)) if items.is_empty() => {
                        timer.finish(true);
                        return Ok(Judged::inconclusive(EMPTY_SELECTION.to_owned()));
                    }
                    (Some(_), _) => {
                        evaluate_item_mode(&requirement.id, operator, expected, &observed)
                    }
                    (None, _) => evaluate_single(&requirement.id, operator, expected, &observed),
                };
                let ok = compared.is_ok();
                (Judged::compared(compared), ok)
            }
            Check::Trace(trace) => {
                match self.trace(&requirement.id, trace, &inputs, deadline).await {
                    Ok(judged) => judged,
                    Err(error) => {
                        timer.finish(false);
                        return Err(error);
                    }
                }
            }
            Check::Agent(_) | Check::Code(_) => {
                let call = BlockCall {
                    id: &requirement.id,
                    position: step
                        .map_or(Position::VerifierCheck, |step| Position::StepCheck { step }),
                    block: &requirement.check,
                    inputs,
                    workspace,
                    deadline,
                };
                match self.checks.execute(call).await {
                    Ok(output) => (Judged::cited(output, &sources), true),
                    Err(BlockError::Busy { reason }) => {
                        timer.finish(false);
                        return Err(EngineError::Busy(reason));
                    }
                    Err(error) => (Judged::inconclusive(reason(&error)), false),
                }
            }
            Check::Drift(drift) => (
                Judged::checked(self.drift.judge(&requirement.id, drift)),
                true,
            ),
        };
        timer.finish(ok);
        Ok(judged)
    }

    /// Compare the selected span value of the declared trace.
    ///
    /// The trace is read once per run, through [`Self::read`]. A trace that
    /// is not supplied or cannot be read is inconclusive; a selector matching
    /// nothing compares `null`. Returns the judgment and whether the check
    /// judged (`false` when it could not read or compare).
    ///
    /// # Errors
    /// Returns [`EngineError::EvidenceUnavailable`] when a deferring engine's
    /// trace has not landed yet.
    async fn trace(
        &self,
        id: &TaskId,
        trace: &TraceCheck,
        inputs: &BTreeMap<String, CallInput>,
        deadline: Instant,
    ) -> Result<(Judged, bool), EngineError> {
        let Some(CallValue::Evidence(InputDeclaration::Trace { trace_id })) =
            inputs.get("on").map(|input| &input.value)
        else {
            return Ok((
                Judged::inconclusive(format!("`{}` supplied no trace", trace.on)),
                false,
            ));
        };
        let spans = match self.read(*trace_id, deadline).await {
            Ok(spans) => spans,
            Err(error @ TraceUnavailable::NotYetLanded { .. }) if self.defer_unlanded => {
                return Err(EngineError::EvidenceUnavailable(error.to_string()));
            }
            Err(error) => {
                tracing::debug!(requirement = id.as_str(), %error, "trace evidence unreadable");
                return Ok((Judged::inconclusive(error.to_string()), false));
            }
        };
        let document = project_spans(*trace_id, &spans);
        let compared = extract_jsonpath_from(&document, &trace.span_selector)
            .and_then(|observed| evaluate_single(id, &trace.operator, &trace.expected, &observed));
        let ok = compared.is_ok();
        Ok((Judged::compared(compared), ok))
    }

    /// The run's one read of `trace_id`: the first check naming it fetches
    /// within `deadline` and every other check of the run, concurrent or
    /// later, shares that result, failure included.
    ///
    /// # Errors
    /// Returns the shared fetch's [`TraceUnavailable`].
    async fn read(
        &self,
        trace_id: TraceId,
        deadline: Instant,
    ) -> Result<Arc<Vec<SpanRecord>>, TraceUnavailable> {
        let cell = Arc::clone(
            self.reads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry(trace_id)
                .or_default(),
        );
        cell.get_or_init(|| async {
            let wait = deadline.saturating_duration_since(Instant::now());
            self.traces.fetch(trace_id, wait).await
        })
        .await
        .clone()
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
fn prepare(scope: &Scope<'_>, attached: &Attached, step: &Step) -> Result<Bindings, Missing> {
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
            mount(scope, attached, &call.workspace, None),
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

/// Resolve the bindings a check reads, with an Agent check's workspace;
/// drift reads its executor's window.
///
/// # Errors
/// Returns the strongest gap among the bindings.
fn check_bindings(
    scope: &Scope<'_>,
    attached: &Attached,
    requirement: &Requirement,
) -> Result<Bindings, Missing> {
    let current = Some(requirement);
    let inputs = match &requirement.check {
        Check::Assert { path, each, .. } => {
            scope.resolve_all(path.iter().chain(each).map(|path| ("value", path)), current)
        }
        Check::Trace(trace) => scope.resolve_all([("on", &trace.on)], current),
        Check::Agent(call) => {
            let (inputs, workspace) = (
                scope.resolve_all(&call.inputs, current),
                mount(scope, attached, &call.workspace, current),
            );
            return match (inputs, workspace) {
                (Ok(inputs), Ok(workspace)) => Ok(Bindings { inputs, workspace }),
                (inputs, workspace) => Err(Missing::strongest(
                    [inputs.err(), workspace.err()].into_iter().flatten(),
                )
                .unwrap_or_else(|| Missing::failed("a binding did not resolve"))),
            };
        }
        Check::Code(check) => scope.resolve_all(&check.call.inputs, current),
        Check::Drift(_) => Ok(BTreeMap::new()),
    }?;
    Ok(Bindings {
        inputs,
        workspace: BTreeMap::new(),
    })
}

/// Resolve an Agent workspace: scope references per call, and each fixed
/// Artifact selection's files at their selected-relative paths beneath the
/// destination, each sourced from its attachment slot.
///
/// # Errors
/// Returns the strongest gap among the scope references, or a failure for
/// a selection the run did not attach.
fn mount(
    scope: &Scope<'_>,
    attached: &Attached,
    workspace: &BTreeMap<String, WorkspaceSource>,
    requirement: Option<&Requirement>,
) -> Result<BTreeMap<String, CallInput>, Missing> {
    let mut files = scope.resolve_all(
        workspace
            .iter()
            .filter_map(|(destination, source)| Some((destination, source.reference()?))),
        requirement,
    )?;
    for (destination, source) in workspace {
        let WorkspaceSource::Artifact { artifact } = source else {
            continue;
        };
        let (_, mounted) = attached
            .iter()
            .find(|(selection, _)| selection == artifact)
            .ok_or_else(|| Missing::failed(format!("{destination} was not attached")))?;
        for (relative, slot, declaration) in mounted {
            let path = if relative.is_empty() {
                destination.clone()
            } else {
                format!("{destination}/{relative}")
            };
            files.insert(
                path,
                CallInput {
                    value: CallValue::Evidence(declaration.clone()),
                    sources: BTreeSet::from([slot.clone()]),
                },
            );
        }
    }
    Ok(files)
}

/// Every fixed Artifact selection of `spec`'s Agent steps and checks, in
/// declaration order.
fn fixed_selections(spec: &VerifierSpec) -> impl Iterator<Item = &ArtifactSelection> {
    let mut calls: Vec<&AgentCall> = Vec::new();
    let requirements = spec
        .steps
        .iter()
        .flat_map(|step| &step.requirements)
        .chain(&spec.requirements);
    for step in &spec.steps {
        if let Action::Agent(call) = &step.action {
            calls.push(call);
        }
    }
    for requirement in requirements {
        if let Check::Agent(call) = &requirement.check {
            calls.push(call);
        }
    }
    calls.into_iter().flat_map(|call| {
        call.workspace.values().filter_map(|source| match source {
            WorkspaceSource::Artifact { artifact } => Some(artifact),
            WorkspaceSource::Scope(_) => None,
        })
    })
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
    /// Custom data an accepted Verdict carried.
    data: Option<Map<String, Value>>,
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
            data: self.data,
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
            data: None,
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
            data: output.verdict.data,
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
/// `media` slot, a trace for a `trace` slot, and an inline JSON list for a
/// group `source` slot; `samples` slots are filled only by a schedule. A JSON value must satisfy its slot schema, and a file
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
            (InputSlot::Source(_), InputDeclaration::Json { value }) => {
                if !value.is_array() {
                    return Err(invalid("a source slot holds one list of source values"));
                }
            }
            _ => return Err(invalid("the declaration is not the slot's kind")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
