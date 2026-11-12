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
//!
//! A queued execution runs with [`Checkpoints`]: each action or check attempt
//! is recorded before it is dispatched and each settled block is committed
//! before its dependents run, so a reclaimed execution restores every
//! settled block instead of running it again and counts every attempt that
//! never settled against the step's retries.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use serde_json::{Map, Value};
use thiserror::Error;
use tokio::sync::Semaphore;
use tokio::time::{Instant, timeout_at};

use wyrd_spec::card::verifier::{
    Action, AgentCall, ArtifactSelection, Check, Condition, Decide, DependsOn, InputSlot,
    Requirement, Step, TraceCheck, VerifierSpec, WorkspaceSource,
};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::VerificationVerdict;
use wyrd_spec::verification::execution::{
    CallProvenance, Citation, ExecutedOutcome, ExecutedRequirement, InputDeclaration,
    InvocationSettlement, Producer, ProducerInvocation, RequirementDetail, RequirementDetails,
    RequirementOutcome, RequirementResult, Speculation, StepResult, StepStatus,
};
use wyrd_spec::verification::state::{ControlRecord, ExecutionState};
use wyrd_spec::verification::{ComparisonOperator, ContextCapture};

use super::BoundJson;
use super::drift::DriftChecks;
use super::scope::{
    Cause, Done, Frame, Iteration, Ledger, Missing, Scope, Settled, StepDone, attempt_records,
};
use super::{
    ActionExecutor, ActionOutput, BlockCall, BlockError, CallInput, CallValue, CheckExecutor,
    CheckOutput, Checkpoints, EvidenceReader, Position,
};
use crate::assertion::capture_actual;
use crate::assertion::{Compared, evaluate_item_mode, evaluate_single};
use crate::error::ExecError;
use crate::jsonpath::extract_jsonpath_from;
use crate::operators;
use crate::trace_source::project_spans;
use crate::trace_source::{TraceSource, TraceUnavailable};
use crate::walk::{Prepared, Tasks};
use wyrd_spec::vala::ids::TraceId;
use wyrd_spec::vala::trace::SpanRecord;

/// Requirement reason of an `each` assertion whose selector matched nothing.
const EMPTY_SELECTION: &str = "empty_selection";

/// Step or requirement reason when the run's `max_agent_invocations` budget
/// is spent.
const BUDGET_SPENT: &str = "the Verifier's max_agent_invocations budget is spent";

/// Step reason when interrupted attempts already spent every retry.
const ATTEMPTS_INTERRUPTED: &str = "the step's attempts were spent by interrupted calls";

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
    /// A block panicked; the rest of its stage was dropped.
    #[error("a verification block task stopped unexpectedly: {0}")]
    Join(String),
    /// A block was refused process capacity at admission
    /// ([`BlockError::Busy`]); unfinished work was cancelled and no verdict
    /// exists, so the host may run the whole execution again later.
    #[error("a verification block was refused process capacity: {0}")]
    Busy(String),
    /// An exact definition a block requires is unavailable
    /// ([`BlockError::Unavailable`]); unfinished work was cancelled, no
    /// verdict exists, and the execution cannot run as declared.
    #[error("a required verification definition is unavailable: {0}")]
    DefinitionUnavailable(String),
    /// A block's invocation record or settlement could not commit, so its
    /// work is not durable; unfinished work was cancelled and the host may
    /// run the execution again.
    #[error("a verification checkpoint cannot commit: {0}")]
    Checkpoint(String),
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
    /// The queued execution's invocation record; `None` runs without one.
    checkpoints: Option<Arc<Checkpointed>>,
    /// The current run's Agent invocation budget; `None` without
    /// `limits.max_agent_invocations`.
    budget: Option<Arc<Budget>>,
    /// The current run's own concurrency bound beneath the engine's;
    /// `None` without `limits.concurrency`.
    run_permits: Option<Arc<Semaphore>>,
    /// The highest attempt the current run began per block and instance;
    /// [`Self::run`] starts each run with none.
    begun: Begun,
}

/// One run's highest begun attempt per block and instance.
type Begun = Arc<std::sync::Mutex<HashMap<(TaskId, Instance), u32>>>;

/// One invocation of a block: its zero-based fanout item and repeat
/// iteration, each `None` when its step does not fan out or repeat.
type Instance = (Option<u32>, Option<u32>);

/// One queued execution's invocation record and what it already holds.
struct Checkpointed {
    /// Where attempts, settlements, and decisions commit.
    store: Arc<dyn Checkpoints>,
    /// Every settled invocation by block and instance.
    settled: HashMap<(TaskId, Instance), InvocationSettlement>,
    /// The highest attempt already recorded per block and instance, settled
    /// or not.
    attempts: HashMap<(TaskId, Instance), u32>,
    /// Every recorded scheduling decision.
    controls: Vec<ControlRecord>,
}

/// One run's Agent invocation budget: every Agent action or check attempt,
/// recorded ones included, spends one of `limit`; nothing replenishes it.
struct Budget {
    /// `limits.max_agent_invocations`.
    limit: u32,
    /// Attempts spent so far.
    spent: AtomicU32,
}

impl Budget {
    /// Spend one invocation, or return `false` when none remains.
    fn spend(&self) -> bool {
        self.spent
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |spent| {
                (spent < self.limit).then_some(spent + 1)
            })
            .is_ok()
    }
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
            checkpoints: None,
            budget: None,
            run_permits: None,
            begun: Begun::default(),
        }
    }

    /// Record every block invocation of this queued execution in `store`,
    /// which already holds `state`: its invocation attempts and scheduling
    /// decisions. A settled invocation is restored instead of running again,
    /// an unsettled attempt counts as spent, and a recorded decision is
    /// reproduced instead of made again.
    #[must_use]
    pub fn with_checkpoints(mut self, store: Arc<dyn Checkpoints>, state: &ExecutionState) -> Self {
        let mut checkpointed = Checkpointed {
            store,
            settled: HashMap::new(),
            attempts: HashMap::new(),
            controls: state.controls.clone(),
        };
        for record in &state.invocations {
            let producer = &record.producer;
            let key = (
                producer.step_id.clone(),
                (producer.item_index, producer.iteration_index),
            );
            let seen = checkpointed.attempts.entry(key.clone()).or_default();
            *seen = (*seen).max(producer.attempt);
            if let Some(settlement) = &record.settlement {
                checkpointed.settled.insert(key, settlement.clone());
            }
        }
        self.checkpoints = Some(Arc::new(checkpointed));
        self
    }

    /// The restored settlement of `task`'s `instance`, when an earlier
    /// attempt of this execution committed one.
    fn restored(&self, task: &TaskId, instance: Instance) -> Option<&InvocationSettlement> {
        self.checkpoints
            .as_ref()?
            .settled
            .get(&(task.clone(), instance))
    }

    /// The restored action settlement of `step`'s `instance`.
    fn restored_action(&self, step: &TaskId, instance: Instance) -> Option<StepDone> {
        match self.restored(step, instance)? {
            InvocationSettlement::Action { result, sources } => {
                Some(own((**result).clone(), sources.clone(), instance))
            }
            InvocationSettlement::Requirement { .. } => None,
        }
    }

    /// Attempts of `task`'s `instance` already recorded by earlier runs of
    /// this execution.
    fn recorded(&self, task: &TaskId, instance: Instance) -> u32 {
        self.checkpoints
            .as_ref()
            .and_then(|checkpointed| {
                checkpointed
                    .attempts
                    .get(&(task.clone(), instance))
                    .copied()
            })
            .unwrap_or(0)
    }

    /// The frozen fanout items of `step`, when an earlier run recorded them.
    fn frozen(&self, step: &TaskId) -> Option<&Vec<Value>> {
        self.checkpoints
            .as_ref()?
            .controls
            .iter()
            .find_map(|control| match control {
                ControlRecord::Items {
                    step: frozen,
                    items,
                } if frozen == step => Some(items),
                _ => None,
            })
    }

    /// The recorded disposition of speculative `step`, when an earlier run
    /// decided it.
    fn disposition(&self, step: &TaskId) -> Option<(Speculation, String)> {
        self.checkpoints
            .as_ref()?
            .controls
            .iter()
            .find_map(|control| match control {
                ControlRecord::Disposition {
                    step: decided,
                    speculation,
                    reason,
                } if decided == step => Some((*speculation, reason.clone())),
                _ => None,
            })
    }

    /// Agent action and check attempts earlier runs of this execution
    /// recorded, which the run's budget has already spent.
    fn agent_attempts(&self, spec: &VerifierSpec) -> u32 {
        let Some(checkpointed) = &self.checkpoints else {
            return 0;
        };
        let agents: BTreeSet<&TaskId> = spec
            .steps
            .iter()
            .filter(|step| matches!(step.action, Action::Agent(_)))
            .map(|step| &step.id)
            .chain(
                spec.steps
                    .iter()
                    .flat_map(|step| &step.requirements)
                    .chain(&spec.requirements)
                    .filter(|requirement| matches!(requirement.check, Check::Agent(_)))
                    .map(|requirement| &requirement.id),
            )
            .collect();
        checkpointed
            .attempts
            .iter()
            .filter(|((task, _), _)| agents.contains(task))
            .map(|(_, attempts)| *attempts)
            .sum()
    }

    /// Spend one Agent invocation of the run's budget, or return `false` when
    /// it is spent; always `true` without a budget.
    fn spend(&self) -> bool {
        self.budget.as_ref().is_none_or(|budget| budget.spend())
    }

    /// Wait for capacity for one block: the run's own bound first, then the
    /// engine's, so a block waiting on its run holds no shared permit.
    async fn admit(
        &self,
    ) -> (
        Option<tokio::sync::OwnedSemaphorePermit>,
        Option<tokio::sync::SemaphorePermit<'_>>,
    ) {
        let run = match &self.run_permits {
            Some(permits) => Arc::clone(permits).acquire_owned().await.ok(),
            None => None,
        };
        (run, self.permits.acquire().await.ok())
    }

    /// Lock the current run's begun attempts, recovering them from a
    /// poisoned lock: no code path panics while holding it.
    fn begun_attempts(&self) -> std::sync::MutexGuard<'_, HashMap<(TaskId, Instance), u32>> {
        self.begun
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Every attempt of `step` that began, in this run or an earlier run
    /// of this execution, ordered by item, iteration, then attempt, for a
    /// contribution withheld before it settled in this run. A restored
    /// settlement reports its own records; every other instance's last
    /// attempt ends as `status` for `reason`. Nothing begun is an empty
    /// list.
    fn attempted(
        &self,
        step: &TaskId,
        status: StepStatus,
        reason: &str,
    ) -> Vec<ProducerInvocation> {
        let mut instances: BTreeMap<Instance, u32> = BTreeMap::new();
        let recorded = self
            .checkpoints
            .iter()
            .flat_map(|checkpointed| &checkpointed.attempts)
            .map(|(key, attempts)| (key.clone(), *attempts));
        let begun: Vec<_> = self
            .begun_attempts()
            .iter()
            .map(|(key, attempts)| (key.clone(), *attempts))
            .collect();
        for ((task, instance), attempts) in recorded.chain(begun) {
            if task == *step && attempts > 0 {
                let seen = instances.entry(instance).or_default();
                *seen = (*seen).max(attempts);
            }
        }
        instances
            .into_iter()
            .flat_map(
                |(instance, attempts)| match self.restored_action(step, instance) {
                    Some(done) => done.invocations,
                    None => attempt_records(instance.0, instance.1, attempts, status, Some(reason))
                        .collect(),
                },
            )
            .collect()
    }

    /// Record that the attempt `producer` starts, before it is dispatched.
    ///
    /// # Errors
    /// Returns [`EngineError::Checkpoint`] when the record cannot commit.
    async fn begin(&self, producer: &Producer) -> Result<(), EngineError> {
        self.begun_attempts()
            .insert(instance_key(producer), producer.attempt);
        match &self.checkpoints {
            Some(checkpointed) => checkpointed
                .store
                .begin(producer)
                .await
                .map_err(EngineError::Checkpoint),
            None => Ok(()),
        }
    }

    /// Forget the unsettled attempt `producer`, refused process capacity or
    /// its definition; the store keeps an attempt that already retained work.
    ///
    /// # Errors
    /// Returns [`EngineError::Checkpoint`] when the record cannot be removed.
    async fn forget(&self, producer: &Producer) -> Result<(), EngineError> {
        self.begun_attempts()
            .insert(instance_key(producer), producer.attempt - 1);
        match &self.checkpoints {
            Some(checkpointed) => checkpointed
                .store
                .forget(producer)
                .await
                .map_err(EngineError::Checkpoint),
            None => Ok(()),
        }
    }

    /// Commit `settled` as the settlement of the attempt `producer`, before
    /// anything reads it.
    ///
    /// # Errors
    /// Returns [`EngineError::Checkpoint`] when the settlement cannot commit.
    async fn commit(
        &self,
        producer: &Producer,
        settled: &InvocationSettlement,
    ) -> Result<(), EngineError> {
        match &self.checkpoints {
            Some(checkpointed) => checkpointed
                .store
                .settle(producer, settled)
                .await
                .map_err(EngineError::Checkpoint),
            None => Ok(()),
        }
    }

    /// Record the scheduling decision `control`, before anything depends on
    /// it.
    ///
    /// # Errors
    /// Returns [`EngineError::Checkpoint`] when the decision cannot commit.
    async fn control(&self, control: &ControlRecord) -> Result<(), EngineError> {
        match &self.checkpoints {
            Some(checkpointed) => checkpointed
                .store
                .control(control)
                .await
                .map_err(EngineError::Checkpoint),
            None => Ok(()),
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
    /// Returns the verdict, every requirement in plan order (each step's
    /// requirement instances by step, in item, iteration, then declaration
    /// order, then whole-Verifier requirements in declaration order), every
    /// step in plan order, and the evidence unchanged.
    ///
    /// The Verifier's `limits` apply to this run only: `concurrency` lowers
    /// the engine's bound, and `max_agent_invocations` counts every Agent
    /// action and check attempt, those earlier runs recorded included.
    ///
    /// The evidence has one owner for the whole run: the engine pins it,
    /// with its synthetic attachment slots, behind one [`Arc`] that the host
    /// reader is admitted with and every block resolves against, and moves
    /// it back out into the outcome once every block has settled.
    ///
    /// The deadline is checked before any work and again before the verdict
    /// is published, so work that completes without suspending, such as an
    /// empty Verifier, never reports a verdict outside its budget; pending
    /// work is cancelled when the deadline passes.
    ///
    /// Before any block runs, every supplied declaration must conform to its
    /// declared slot (see [`conform`]) and the host's evidence reader, when
    /// set, must admit the whole set. A declared slot left unsupplied is not
    /// refused here: it leaves the checks that need it inconclusive.
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
    /// capacity, and [`EngineError::DefinitionUnavailable`] when one's exact
    /// definition is unavailable.
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
    ) -> Result<ExecutedOutcome, EngineError> {
        let expired = || Instant::now() >= deadline;
        if expired() {
            return Err(EngineError::DeadlineExceeded);
        }
        conform(&spec, &evidence)?;
        let plan = spec.plan().map_err(EngineError::Plan)?;
        let limits = spec.limits.as_ref();
        let (ledger, evidence) = timeout_at(deadline, async {
            let (evidence, attached) = self.attach(&spec, evidence).await?;
            let evidence = Arc::new(evidence);
            if let Some(reader) = &self.evidence {
                reader.admit(&spec.inputs, &evidence).await?;
            }
            let attempt = Self {
                reads: TraceReads::default(),
                begun: Begun::default(),
                attached: Arc::new(attached),
                budget: limits
                    .and_then(|limits| limits.max_agent_invocations)
                    .map(|limit| {
                        Arc::new(Budget {
                            limit,
                            spent: AtomicU32::new(self.agent_attempts(&spec)),
                        })
                    }),
                run_permits: limits
                    .and_then(|limits| limits.concurrency)
                    .map(|bound| Arc::new(Semaphore::new(bound as usize))),
                ..self.clone()
            };
            let ledger = attempt.drive(&spec, &evidence, deadline).await?;
            Ok::<_, EngineError>((ledger, evidence))
        })
        .await
        .map_err(|_| EngineError::DeadlineExceeded)??;
        // Every block has settled, so each settlement has one owner left and
        // moves into the outcome, its shared step output uncopied.
        let mut settled: HashMap<TaskId, Done> = ledger
            .into_iter()
            .map(|(id, done)| (id, Arc::unwrap_or_clone(done)))
            .collect();
        let (mut steps, mut requirements) = (Vec::new(), Vec::new());
        for id in plan.stages.iter().flat_map(|stage| &stage.tasks) {
            if let Some(Done::Step(done)) = settled.remove(id) {
                let StepDone {
                    mut result,
                    state,
                    requirements: instances,
                    ..
                } = *done;
                if let Settled::Succeeded { output, .. } = state {
                    result.output = Some(Arc::unwrap_or_clone(output));
                }
                steps.push(result);
                requirements.extend(instances);
            }
        }
        for requirement in &spec.requirements {
            if let Some(Done::Requirement(result)) = settled.remove(&requirement.id) {
                requirements.push(*result);
            }
        }
        let verdict = decide(
            spec.decide,
            requirements
                .iter()
                .map(|requirement| requirement.result.outcome),
        );
        if expired() {
            return Err(EngineError::DeadlineExceeded);
        }
        Ok(ExecutedOutcome {
            verdict,
            requirements,
            steps,
            // Every block has settled and readers keep only a weak view, so
            // this is the last owner and the evidence moves out uncopied.
            evidence: Arc::unwrap_or_clone(evidence),
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

    /// Release every block as soon as the blocks it depends on are complete,
    /// until every step and whole-Verifier requirement has settled.
    ///
    /// A step is ready once each of its [`Step::prerequisites`] is complete,
    /// and, for `any_succeeded`, once one candidate succeeded or every
    /// candidate is complete. A step is complete once it settled and, when
    /// speculative, its disposition is decided. Whole-Verifier requirements
    /// are ready once every step is complete. A speculative step's guard is
    /// decided as soon as its guard steps complete: a running step the guard
    /// does not admit is cancelled, and a settled one's disposition is
    /// recorded before anything reads it.
    ///
    /// # Errors
    /// Returns [`EngineError::Capture`], [`EngineError::Join`],
    /// [`EngineError::Busy`], [`EngineError::DefinitionUnavailable`], or
    /// [`EngineError::Checkpoint`]; every running block is then dropped,
    /// except that a capacity refusal ([`EngineError::Busy`]) first lets every
    /// running block settle and dispatches nothing more.
    async fn drive(
        &self,
        spec: &Arc<VerifierSpec>,
        evidence: &Arc<BTreeMap<SlotName, InputDeclaration>>,
        deadline: Instant,
    ) -> Result<Ledger, EngineError> {
        let mut ledger = Ledger::default();
        let mut pending: Vec<Node> = (0..spec.steps.len())
            .map(Node::Step)
            .chain((0..spec.requirements.len()).map(Node::Requirement))
            .collect();
        let mut tasks: Tasks<Node, Arc<Done>, EngineError> = Tasks::default();
        let mut disposed = BTreeSet::new();
        loop {
            let mut released = true;
            while released {
                released = false;
                for (index, step) in spec.steps.iter().enumerate() {
                    if !step.speculative || disposed.contains(&index) {
                        continue;
                    }
                    let scope = Scope::new(evidence, &ledger);
                    let Some((speculation, reason)) = guard(&scope, &ledger, step) else {
                        continue;
                    };
                    let settled = match ledger.get(&step.id).map(AsRef::as_ref) {
                        Some(Done::Step(done)) if done.speculation == Speculation::Pending => {
                            (**done).clone()
                        }
                        None if tasks.contains(&Node::Step(index)) && !speculation.admits() => {
                            tasks.cancel(&Node::Step(index));
                            let status = withheld(speculation, &reason).0;
                            let mut cancelled =
                                unrun(step, Missing::failed(reason.clone()), Speculation::Pending);
                            cancelled.invocations = self.attempted(&step.id, status, &reason);
                            cancelled
                        }
                        _ => continue,
                    };
                    let decided =
                        self.decide_speculation(spec, &scope, index, settled, speculation, &reason);
                    disposed.insert(index);
                    let engine = self.clone();
                    let control = ControlRecord::Disposition {
                        step: step.id.clone(),
                        speculation,
                        reason,
                    };
                    tasks.push(
                        Node::Disposition(index),
                        Box::pin(async move {
                            engine.control(&control).await?;
                            Ok(Arc::new(Done::Step(Box::new(decided))))
                        }),
                    );
                }
                let mut at = 0;
                while at < pending.len() {
                    if !ready(spec, &ledger, pending[at]) {
                        at += 1;
                        continue;
                    }
                    let node = pending.remove(at);
                    released = true;
                    match self.prepare(spec, evidence, &ledger, node, deadline)? {
                        Prepared::Settled(done) => ledger.insert(node.id(spec).clone(), done),
                        Prepared::Run(task) => tasks.push(node, task),
                    }
                }
            }
            if tasks.is_empty() {
                // Validation keeps the graph acyclic, so nothing stays
                // pending once nothing runs; bind the first anyway rather
                // than wait forever, so its gaps settle it.
                if pending.is_empty() {
                    return Ok(ledger);
                }
                let node = pending.remove(0);
                match self.prepare(spec, evidence, &ledger, node, deadline)? {
                    Prepared::Settled(done) => ledger.insert(node.id(spec).clone(), done),
                    Prepared::Run(task) => tasks.push(node, task),
                }
                continue;
            }
            let (node, done) = tasks.next(EngineError::Join).await;
            ledger.insert(node.id(spec).clone(), settled(done, &mut tasks).await?);
        }
    }

    /// Prepare one ready `node` against the blocks already in `ledger`.
    ///
    /// # Errors
    /// Returns [`EngineError::Capture`] when a requirement's actual value
    /// cannot be captured.
    fn prepare(
        &self,
        spec: &Arc<VerifierSpec>,
        evidence: &Arc<BTreeMap<SlotName, InputDeclaration>>,
        ledger: &Ledger,
        node: Node,
        deadline: Instant,
    ) -> Result<Prepared<Arc<Done>, EngineError>, EngineError> {
        match node {
            Node::Step(index) | Node::Disposition(index) => {
                self.step(spec, evidence, ledger, index, deadline)
            }
            Node::Requirement(at) => {
                let scope = Scope::new(evidence, ledger);
                Ok(
                    match self.requirement(spec, &scope, (None, at), (None, None), deadline) {
                        Prepared::Settled(result) => {
                            Prepared::Settled(Arc::new(Done::Requirement(Box::new(result))))
                        }
                        Prepared::Run(task) => Prepared::Run(Box::pin(async move {
                            Ok(Arc::new(Done::Requirement(Box::new(task.await?))))
                        })),
                    },
                )
            }
        }
    }

    /// The settlement of speculative step `index` once its guard decided
    /// `speculation`: an admitted contribution is kept, any other is
    /// withheld, its output and requirement judgments replaced by the
    /// guard's gap while its invocations stay reported.
    fn decide_speculation(
        &self,
        spec: &VerifierSpec,
        scope: &Scope<'_>,
        index: usize,
        mut done: StepDone,
        speculation: Speculation,
        reason: &str,
    ) -> StepDone {
        done.speculation = speculation;
        if speculation.admits() {
            return done;
        }
        let (status, gap) = withheld(speculation, reason);
        done.result.status = status;
        done.result.attempts = u32::try_from(done.invocations.len()).unwrap_or(u32::MAX);
        done.result.output = None;
        done.result.reason = Some(gap.reason.clone());
        done.state = Settled::NoOutput { status, gap };
        done.member = None;
        done.requirements = missing_requirements(spec, scope, &self.attached, index, &done);
        done
    }

    /// Prepare one step.
    ///
    /// In order: an `any_succeeded` step none of whose candidates succeeded
    /// is skipped with their strongest gap; an ordinary `when` is decided
    /// before anything else binds, a false one skipping the step and a gap
    /// or unevaluable one skipping it with that cause; a speculative `when`
    /// whose guard steps are complete, or whose recorded disposition exists,
    /// decides the disposition, and withholds the step without a call unless
    /// it admits. Then `foreach.items` is frozen: the recorded items, else the
    /// resolved array, refused without a call when it is not an array or
    /// exceeds `max_items`. Every member's first bindings must resolve, or the
    /// step is skipped with the strongest gap. Otherwise every member runs.
    ///
    /// # Errors
    /// Returns [`EngineError::Capture`] when a requirement's actual value
    /// cannot be captured.
    ///
    /// # Panics
    /// Panics when `index` is not a declared step of `spec`.
    fn step(
        &self,
        spec: &Arc<VerifierSpec>,
        evidence: &Arc<BTreeMap<SlotName, InputDeclaration>>,
        ledger: &Ledger,
        index: usize,
        deadline: Instant,
    ) -> Result<Prepared<Arc<Done>, EngineError>, EngineError> {
        let step = &spec.steps[index];
        let scope = Scope::new(evidence, ledger);
        let settle = |status: StepStatus, gap: Missing, speculation: Speculation| {
            let mut done = unrun(step, gap, speculation);
            done.result.status = status;
            if let Settled::NoOutput {
                status: settled, ..
            } = &mut done.state
            {
                *settled = status;
            }
            done.requirements = missing_requirements(spec, &scope, &self.attached, index, &done);
            Ok(Prepared::Settled(Arc::new(Done::Step(Box::new(done)))))
        };
        if let DependsOn::AnySucceeded { any_succeeded } = &step.depends_on
            && !any_succeeded
                .iter()
                .any(|candidate| matches!(scope.step(candidate), Some(Settled::Succeeded { .. })))
        {
            let gap = Missing::strongest(any_succeeded.iter().filter_map(|candidate| {
                match scope.step(candidate) {
                    Some(Settled::NoOutput { gap, .. }) => Some(gap.clone()),
                    _ => None,
                }
            }))
            .unwrap_or_else(|| Missing::failed("no candidate step succeeded"));
            return settle(StepStatus::Skipped, gap, Speculation::None);
        }
        let mut speculation = Speculation::None;
        if let Some(when) = &step.when {
            if step.speculative {
                match self
                    .disposition(&step.id)
                    .or_else(|| guard(&scope, ledger, step))
                {
                    None => speculation = Speculation::Pending,
                    Some((Speculation::Admitted, _)) => speculation = Speculation::Admitted,
                    Some((decided, reason)) => {
                        let (status, gap) = withheld(decided, &reason);
                        let invocations = self.attempted(&step.id, status, &reason);
                        let mut done = unrun(step, gap, decided);
                        done.result.status = status;
                        done.result.attempts = u32::try_from(invocations.len()).unwrap_or(u32::MAX);
                        done.invocations = invocations;
                        if let Settled::NoOutput {
                            status: settled, ..
                        } = &mut done.state
                        {
                            *settled = status;
                        }
                        done.requirements =
                            missing_requirements(spec, &scope, &self.attached, index, &done);
                        return Ok(Prepared::Settled(Arc::new(Done::Step(Box::new(done)))));
                    }
                }
            } else {
                match condition(&scope, when, "when") {
                    Ok(true) => {}
                    Ok(false) => {
                        let gap = Missing {
                            cause: Cause::Skipped,
                            reason: format!("when `{}` was false", when.path),
                        };
                        return settle(StepStatus::Skipped, gap, speculation);
                    }
                    Err(gap) => return settle(gap.control_status(), gap, speculation),
                }
            }
        }
        let (members, freeze) = match step.foreach.as_ref() {
            None => (vec![None], None),
            Some(foreach) => match (&foreach.items, foreach.count) {
                (Some(items), _) => {
                    let resolved = match scope.resolve(items, None) {
                        Ok(resolved) => resolved,
                        Err(gap) => return settle(gap.control_status(), gap, speculation),
                    };
                    let selected = match &resolved.value {
                        CallValue::Json(value) => value.as_array(),
                        _ => None,
                    };
                    let (values, freeze) = match (self.frozen(&step.id), selected) {
                        (Some(frozen), _) => (frozen.clone(), None),
                        (None, Some(values)) => {
                            let values = values.clone();
                            let freeze = ControlRecord::Items {
                                step: step.id.clone(),
                                items: values.clone(),
                            };
                            (values, Some(freeze))
                        }
                        (None, _) => {
                            let gap = Missing::failed(format!("`{items}` is not an array"));
                            return settle(StepStatus::Failed, gap, speculation);
                        }
                    };
                    if let Some(max) = foreach.max_items.filter(|max| values.len() > *max as usize)
                    {
                        let gap = Missing::failed(format!(
                            "`{items}` holds {} items, more than max_items {max}",
                            values.len()
                        ));
                        return settle(StepStatus::Failed, gap, speculation);
                    }
                    let members = (0_u32..)
                        .zip(values)
                        .map(|(at, value)| {
                            Some((
                                at,
                                Some(CallInput {
                                    value: CallValue::Json(value.into()),
                                    sources: resolved.sources.clone(),
                                }),
                            ))
                        })
                        .collect();
                    (members, freeze)
                }
                (None, count) => (
                    (0..count.unwrap_or(0)).map(|at| Some((at, None))).collect(),
                    None,
                ),
            },
        };
        let first = |item: &Option<(u32, Option<CallInput>)>| Frame {
            item: item.clone(),
            iteration: step.repeat.as_ref().map(|_| Iteration {
                index: 0,
                previous: Value::Null,
                output: None,
            }),
            own: None,
        };
        let gaps: Vec<Missing> = members
            .iter()
            .filter_map(|item| {
                let scope = Scope::new(evidence, ledger).framed(first(item));
                bindings(&scope, &self.attached, step).err()
            })
            .collect();
        if let Some(gap) = Missing::strongest(gaps) {
            return settle(StepStatus::Skipped, gap, speculation);
        }
        let engine = self.clone();
        let spec = Arc::clone(spec);
        let evidence = Arc::clone(evidence);
        let ledger = Arc::new(ledger.clone());
        Ok(Prepared::Run(Box::pin(async move {
            if let Some(freeze) = &freeze {
                engine.control(freeze).await?;
            }
            let started = Instant::now();
            let fanout = spec.steps[index].foreach.is_some();
            let members = engine
                .members(&spec, &evidence, &ledger, index, members, deadline)
                .await?;
            let mut done = aggregate(&spec.steps[index], members, fanout, started.elapsed());
            done.speculation = speculation;
            Ok(Arc::new(Done::Step(Box::new(done))))
        })))
    }

    /// Run every member of step `index` concurrently, at most
    /// `foreach.concurrency` at once, returning them in item order.
    ///
    /// # Errors
    /// Returns the first orchestration failure a member returns; every other
    /// member is then dropped, except that a capacity refusal first lets
    /// every running member settle.
    ///
    /// # Panics
    /// Panics when `index` does not name a step of `spec`.
    async fn members(
        &self,
        spec: &Arc<VerifierSpec>,
        evidence: &Arc<BTreeMap<SlotName, InputDeclaration>>,
        ledger: &Arc<Ledger>,
        index: usize,
        items: Vec<Option<(u32, Option<CallInput>)>>,
        deadline: Instant,
    ) -> Result<Vec<Member>, EngineError> {
        let bound = spec.steps[index]
            .foreach
            .as_ref()
            .and_then(|foreach| foreach.concurrency)
            .map(|bound| Arc::new(Semaphore::new(bound as usize)));
        let mut running: Tasks<usize, Member, EngineError> = Tasks::default();
        let total = items.len();
        for (at, item) in items.into_iter().enumerate() {
            let (engine, spec, evidence, ledger, bound) = (
                self.clone(),
                Arc::clone(spec),
                Arc::clone(evidence),
                Arc::clone(ledger),
                bound.clone(),
            );
            running.push(
                at,
                Box::pin(async move {
                    let _held = match bound {
                        Some(bound) => bound.acquire_owned().await.ok(),
                        None => None,
                    };
                    engine
                        .member(&spec, &evidence, &ledger, index, item, deadline)
                        .await
                }),
            );
        }
        let mut members: Vec<(usize, Member)> = Vec::with_capacity(total);
        while !running.is_empty() {
            let (at, member) = running.next(EngineError::Join).await;
            members.push((at, settled(member, &mut running).await?));
        }
        members.sort_by_key(|(at, _)| *at);
        Ok(members.into_iter().map(|(_, member)| member).collect())
    }

    /// Run one member of step `index`: its iterations one at a time, each
    /// action then its requirement instances, then `repeat.until`.
    ///
    /// A restored iteration settlement is reused rather than run again. An
    /// action failure ends the member; `until` true stops it successfully,
    /// false continues with the output as `iteration.previous`, and an
    /// unevaluable `until` or an exhausted `max_iterations` fails it.
    ///
    /// # Errors
    /// Returns [`EngineError::Busy`], [`EngineError::DefinitionUnavailable`],
    /// [`EngineError::Checkpoint`], [`EngineError::Capture`], or
    /// [`EngineError::EvidenceUnavailable`] from its blocks.
    ///
    /// # Panics
    /// Panics when `index` does not name a step of `spec`.
    async fn member(
        &self,
        spec: &Arc<VerifierSpec>,
        evidence: &Arc<BTreeMap<SlotName, InputDeclaration>>,
        ledger: &Ledger,
        index: usize,
        item: Option<(u32, Option<CallInput>)>,
        deadline: Instant,
    ) -> Result<Member, EngineError> {
        let step = &spec.steps[index];
        let item_index = item.as_ref().map(|(at, _)| *at);
        let iterations = step
            .repeat
            .as_ref()
            .map_or(1, |repeat| repeat.max_iterations);
        let (mut requirements, mut invocations) = (Vec::new(), Vec::new());
        let mut previous = Value::Null;
        let mut last = None;
        for iteration in 0..iterations {
            let instance = (item_index, step.repeat.as_ref().map(|_| iteration));
            let mut frame = Frame {
                item: item.clone(),
                iteration: step.repeat.as_ref().map(|_| Iteration {
                    index: iteration,
                    previous: previous.clone(),
                    output: None,
                }),
                own: None,
            };
            let mut own = if let Some(own) = self.restored_action(&step.id, instance) {
                own
            } else {
                let scope = Scope::new(evidence, ledger).framed(frame.clone());
                match bindings(&scope, &self.attached, step) {
                    Err(gap) => {
                        let mut own = unrun(step, gap, Speculation::None);
                        own.result.status = StepStatus::Failed;
                        own
                    }
                    Ok(bindings) => {
                        let attempted = self.attempt(step, &bindings, deadline, instance).await?;
                        let attempts = attempted.attempts;
                        let own = attempted.settle(step, bindings.sources(), instance);
                        // Only a checkpointed run records the settlement, so only
                        // it copies the output into one.
                        if attempts > 0 && self.checkpoints.is_some() {
                            let producer = producer(&step.id, instance, attempts);
                            let settled = InvocationSettlement::Action {
                                result: Box::new(reported(&own)),
                                sources: sources(&own.state),
                            };
                            self.commit(&producer, &settled).await?;
                        }
                        own
                    }
                }
            };
            invocations.extend(own.invocations.iter().cloned());
            frame.own = Some(own.clone());
            own.requirements = self
                .instance_requirements(spec, evidence, ledger, index, &frame, deadline)
                .await?;
            requirements.extend(own.requirements.iter().cloned());
            let output = match &own.state {
                Settled::Succeeded { output, sources } => CallInput {
                    value: CallValue::Json(
                        BoundJson::output(output, &[])
                            .expect("an empty path selects the whole output"),
                    ),
                    sources: sources.clone(),
                },
                Settled::NoOutput { .. } => {
                    last = Some(own);
                    break;
                }
            };
            let Some(repeat) = &step.repeat else {
                last = Some(own);
                break;
            };
            frame.own = Some(own.clone());
            if let Some(current) = &mut frame.iteration {
                current.output = Some(output.clone());
            }
            let scope = Scope::new(evidence, ledger).framed(frame);
            let stop = condition(&scope, &repeat.until, "until");
            let fail = |own: &mut StepDone, reason: String| {
                own.result.status = StepStatus::Failed;
                own.result.output = None;
                own.result.reason = Some(reason.clone());
                own.state = Settled::NoOutput {
                    status: StepStatus::Failed,
                    gap: Missing::failed(reason),
                };
                own.member = None;
            };
            match stop {
                Ok(true) => {
                    last = Some(own);
                    break;
                }
                Ok(false) if iteration + 1 == iterations => {
                    fail(
                        &mut own,
                        format!(
                            "until `{}` did not hold within max_iterations {iterations}",
                            repeat.until.path
                        ),
                    );
                    last = Some(own);
                }
                Ok(false) => previous = json(&output).into_owned(),
                Err(gap) => {
                    fail(&mut own, gap.reason);
                    last = Some(own);
                    break;
                }
            }
        }
        let last = last.unwrap_or_else(|| {
            unrun(
                step,
                Missing::failed("the step ran no iteration"),
                Speculation::None,
            )
        });
        Ok(Member {
            item: item_index,
            last,
            requirements,
            invocations,
        })
    }

    /// Judge every requirement of step `index` for the invocation `frame`,
    /// concurrently, returning them in declaration order, each attributed to
    /// the frame's item and iteration.
    ///
    /// A capacity refusal waits for the other dispatched checks to settle
    /// before it returns, so their accepted settlements are kept.
    ///
    /// # Errors
    /// Returns the first orchestration failure a judgment returns.
    ///
    /// # Panics
    /// Panics when `index` does not name a step of `spec`.
    async fn instance_requirements(
        &self,
        spec: &Arc<VerifierSpec>,
        evidence: &Arc<BTreeMap<SlotName, InputDeclaration>>,
        ledger: &Ledger,
        index: usize,
        frame: &Frame,
        deadline: Instant,
    ) -> Result<Vec<ExecutedRequirement>, EngineError> {
        let instance = frame.instance();
        let mut results = Vec::new();
        let mut running: Tasks<usize, ExecutedRequirement, EngineError> = Tasks::default();
        for at in 0..spec.steps[index].requirements.len() {
            let scope = Scope::new(evidence, ledger).framed(frame.clone());
            match self.requirement(spec, &scope, (Some(index), at), instance, deadline) {
                Prepared::Settled(result) => results.push((at, result)),
                Prepared::Run(task) => running.push(at, task),
            }
        }
        while !running.is_empty() {
            let (at, result) = running.next(EngineError::Join).await;
            results.push((at, settled(result, &mut running).await?));
        }
        results.sort_by_key(|(at, _)| *at);
        Ok(results.into_iter().map(|(_, result)| result).collect())
    }

    /// Prepare one requirement at `key` for the owner's `instance`.
    ///
    /// Its evidence gaps are the owning invocation's missing output, if any,
    /// and every explicit binding gap; the strongest cause wins, so a failed
    /// binding is never hidden behind an intentionally skipped owner. A
    /// requirement with a gap takes the gap's outcome without calling its
    /// check. Otherwise the prepared check first waits for capacity, the run's
    /// bound then the engine's, and only once admitted spends an Agent
    /// invocation and records its attempt, so a check waiting for capacity
    /// spends nothing; an Agent check finding the run's budget spent is
    /// inconclusive without a call.
    ///
    /// The prepared check fails with [`EngineError::Busy`] when it is refused
    /// process capacity, [`EngineError::DefinitionUnavailable`] when its
    /// exact definition is unavailable, [`EngineError::Capture`] when its
    /// actual value cannot be hashed, and the checkpoint store's failure.
    ///
    /// # Panics
    /// Panics when `key` does not name a requirement of `spec`.
    fn requirement(
        &self,
        spec: &Arc<VerifierSpec>,
        scope: &Scope<'_>,
        key: Key,
        instance: Instance,
        deadline: Instant,
    ) -> Prepared<ExecutedRequirement, EngineError> {
        let requirement = requirement_at(spec, key);
        if let Some(InvocationSettlement::Requirement { result }) =
            self.restored(&requirement.id, instance)
        {
            return Prepared::Settled((**result).clone());
        }
        let inputs = match requirement_bindings(spec, scope, &self.attached, key) {
            Ok(inputs) => inputs,
            Err(gap) => {
                return Prepared::Settled(Judged::missing(gap).report(spec, key, instance));
            }
        };
        let engine = self.clone();
        let spec = Arc::clone(spec);
        Prepared::Run(Box::pin(async move {
            let requirement = requirement_at(&spec, key);
            let _permits = engine.admit().await;
            if matches!(requirement.check, Check::Agent(_)) && !engine.spend() {
                return Ok(
                    Judged::inconclusive(BUDGET_SPENT.to_owned()).report(&spec, key, instance)
                );
            }
            let attempt = engine.recorded(&requirement.id, instance) + 1;
            let producer = producer(&requirement.id, instance, attempt);
            engine.begin(&producer).await?;
            let step = key.0.map(|index| &spec.steps[index].id);
            let capture = spec.context_capture.unwrap_or(ContextCapture::Full);
            let judged = match engine
                .judge(requirement, step, inputs, capture, deadline, &producer)
                .await
            {
                Err(error @ (EngineError::Busy(_) | EngineError::DefinitionUnavailable(_))) => {
                    engine.forget(&producer).await?;
                    return Err(error);
                }
                judged => judged?,
            };
            let result = judged.report(&spec, key, instance);
            // Only a checkpointed run records the settlement, so only it
            // copies the judged actual value into one.
            if engine.checkpoints.is_some() {
                let settled = InvocationSettlement::Requirement {
                    result: Box::new(result.clone()),
                };
                engine.commit(&producer, &settled).await?;
            }
            Ok(result)
        }))
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
    /// On a checkpointed execution every attempt of the invocation
    /// `instance` is recorded before it is dispatched and counts with the
    /// attempts earlier runs recorded, so an invocation whose interrupted
    /// attempts already spent its retries fails without dispatching again.
    /// An Agent action finding the run's invocation budget spent fails
    /// without dispatching, with reason [`BUDGET_SPENT`].
    ///
    /// # Errors
    /// Returns [`EngineError::Busy`] as soon as an attempt is refused process
    /// capacity and [`EngineError::DefinitionUnavailable`] as soon as its
    /// exact definition is unavailable, either without spending the step's
    /// retries or settling the step, and [`EngineError::Checkpoint`] when an
    /// attempt cannot be recorded.
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
        instance: Instance,
    ) -> Result<Attempted, EngineError> {
        let mut attempts = self.recorded(&step.id, instance);
        if attempts > step.retries {
            return Ok(Attempted {
                attempts,
                duration: Duration::ZERO,
                result: Err(ATTEMPTS_INTERRUPTED.to_owned()),
            });
        }
        let _permits = self.admit().await;
        let started = Instant::now();
        let agent = matches!(step.action, Action::Agent(_));
        loop {
            if agent && !self.spend() {
                return Ok(Attempted {
                    attempts,
                    duration: started.elapsed(),
                    result: Err(BUDGET_SPENT.to_owned()),
                });
            }
            attempts += 1;
            let producer = producer(&step.id, instance, attempts);
            self.begin(&producer).await?;
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
                attempt: attempts,
                item_index: instance.0,
                iteration_index: instance.1,
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
                Err(BlockError::Busy { reason }) => {
                    self.forget(&producer).await?;
                    return Err(EngineError::Busy(reason));
                }
                Err(BlockError::Unavailable { reason }) => {
                    self.forget(&producer).await?;
                    return Err(EngineError::DefinitionUnavailable(reason));
                }
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

    /// Run the attempt `producer` of one requirement's check over its
    /// resolved bindings.
    ///
    /// A failed Agent or code check is inconclusive, and so is an `each`
    /// assertion selecting no value, with reason [`EMPTY_SELECTION`], so an
    /// empty selection never passes vacuously.
    ///
    /// The caller holds the check's permits. The check is timed into
    /// [`BLOCK_DURATION_SECONDS`] inside a debug `verification.block` span; a
    /// check that could
    /// not judge (an uncomparable value, an unreadable trace, a failed or
    /// refused executor) records `outcome="error"`.
    ///
    /// # Errors
    /// Returns [`EngineError::Busy`] when an Agent or code check is refused
    /// process capacity and [`EngineError::DefinitionUnavailable`] when its
    /// exact definition is unavailable, neither of which is judged, and
    /// [`EngineError::EvidenceUnavailable`] when a deferring engine's trace
    /// has not landed yet, and [`EngineError::Capture`] when the actual value
    /// cannot be captured under `capture`.
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
        capture: ContextCapture,
        deadline: Instant,
        producer: &Producer,
    ) -> Result<Judged, EngineError> {
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
                let observed = inputs.get("value").map_or(Cow::Owned(Value::Null), json);
                let compared = match (each, &*observed) {
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
                (Judged::compared(compared, capture), ok)
            }
            Check::Trace(trace) => {
                match self
                    .trace(&requirement.id, trace, &inputs, capture, deadline)
                    .await
                {
                    Ok((judged, ok)) => (Ok(judged), ok),
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
                    attempt: producer.attempt,
                    item_index: producer.item_index,
                    iteration_index: producer.iteration_index,
                };
                match self.checks.execute(call).await {
                    Ok(output) => (Judged::cited(output, &sources, capture), true),
                    Err(BlockError::Busy { reason }) => {
                        timer.finish(false);
                        return Err(EngineError::Busy(reason));
                    }
                    Err(BlockError::Unavailable { reason }) => {
                        timer.finish(false);
                        return Err(EngineError::DefinitionUnavailable(reason));
                    }
                    Err(error) => (Ok(Judged::inconclusive(reason(&error))), false),
                }
            }
            Check::Drift(drift) => (
                Judged::checked(self.drift.judge(&requirement.id, drift), capture),
                true,
            ),
        };
        timer.finish(ok);
        Ok(judged?)
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
        capture: ContextCapture,
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
        let observed = match extract_jsonpath_from(&document, &trace.span_selector) {
            Ok(observed) => observed,
            Err(error) => return Ok((Judged::compared(Err(error), capture)?, false)),
        };
        let compared = evaluate_single(id, &trace.operator, &trace.expected, &observed);
        let ok = compared.is_ok();
        Ok((Judged::compared(compared, capture)?, ok))
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

/// Resolve a step's action bindings for one invocation.
///
/// # Errors
/// Returns the strongest gap among the action bindings, or a failure for an
/// extract source without evidence.
fn bindings(scope: &Scope<'_>, attached: &Attached, step: &Step) -> Result<Bindings, Missing> {
    let empty = || Ok(BTreeMap::new());
    let (inputs, workspace) = match &step.action {
        Action::Extract { from } => (scope.resolve_all([("from", from)], None), empty()),
        Action::Agent(call) => (
            scope.resolve_all(&call.inputs, None),
            mount(scope, attached, &call.workspace, None),
        ),
        Action::Code(call) => (scope.resolve_all(&call.inputs, None), empty()),
    };
    let (inputs, workspace) = match (inputs, workspace) {
        (Ok(inputs), Ok(workspace)) => (inputs, workspace),
        (inputs, workspace) => {
            return Err(
                Missing::strongest([inputs.err(), workspace.err()].into_iter().flatten())
                    .unwrap_or_else(|| Missing::failed("a binding did not resolve")),
            );
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
    Ok(Bindings { inputs, workspace })
}

/// Evaluate a step's `when` or `until` (`clause`) against `scope`. An
/// equality against a value of another JSON type, such as a `null`
/// decision against `true`, cannot be evaluated rather than being false.
///
/// # Errors
/// Returns the reference's gap, or a failure when the comparison cannot be
/// evaluated: a false condition is `Ok(false)`, never an error.
fn condition(scope: &Scope<'_>, condition: &Condition, clause: &str) -> Result<bool, Missing> {
    let path = &condition.path;
    let resolved = scope.resolve(path, None)?;
    let observed = json(&resolved);
    let compared = if matches!(
        condition.operator,
        ComparisonOperator::Equals | ComparisonOperator::NotEquals
    ) && operators::json_kind(&observed)
        != operators::json_kind(&condition.expected)
    {
        Err(operators::mismatch(
            &condition.operator,
            &observed,
            &condition.expected,
        ))
    } else {
        operators::evaluate_operator(&observed, &condition.operator, &condition.expected)
    };
    compared
        .map(|verdict| verdict.passed)
        .map_err(|error| Missing::failed(format!("{clause} `{path}` cannot be evaluated: {error}")))
}

/// The disposition a speculative `step`'s guard decides, once every guard
/// step is complete: admitted when `when` holds, discarded when it is false,
/// and blocked when it cannot be evaluated; `None` while a guard step is
/// incomplete.
fn guard(scope: &Scope<'_>, ledger: &Ledger, step: &Step) -> Option<(Speculation, String)> {
    let when = step.when.as_ref()?;
    if !step
        .guard_steps()
        .into_iter()
        .all(|guard| complete(ledger, guard))
    {
        return None;
    }
    Some(match condition(scope, when, "when") {
        Ok(true) => (Speculation::Admitted, format!("when `{}` held", when.path)),
        Ok(false) => (
            Speculation::Discarded,
            format!("when `{}` was false", when.path),
        ),
        Err(gap) => (Speculation::Blocked, gap.reason),
    })
}

/// The status and gap a withheld speculative contribution settles with: a
/// discarded one is skipped, a blocked one failed.
fn withheld(speculation: Speculation, reason: &str) -> (StepStatus, Missing) {
    match speculation {
        Speculation::Discarded => (
            StepStatus::Skipped,
            Missing {
                cause: Cause::Skipped,
                reason: reason.to_owned(),
            },
        ),
        _ => (StepStatus::Failed, Missing::failed(reason)),
    }
}

/// Whether `task` settled and, when it is a speculative step, its
/// disposition is decided.
fn complete(ledger: &Ledger, task: &TaskId) -> bool {
    match ledger.get(task).map(AsRef::as_ref) {
        Some(Done::Step(done)) => done.speculation != Speculation::Pending,
        Some(Done::Requirement(_)) => true,
        None => false,
    }
}

/// Whether the pending `node` may start against `ledger`.
///
/// # Panics
/// Panics when a [`Node::Step`] index does not name a step of `spec`.
fn ready(spec: &VerifierSpec, ledger: &Ledger, node: Node) -> bool {
    match node {
        Node::Step(index) => {
            let step = &spec.steps[index];
            let candidates = match &step.depends_on {
                DependsOn::AnySucceeded { any_succeeded } => {
                    any_succeeded.iter().any(|candidate| {
                        complete(ledger, candidate)
                            && matches!(
                                ledger.get(candidate).map(AsRef::as_ref),
                                Some(Done::Step(done))
                                    if matches!(done.state, Settled::Succeeded { .. })
                            )
                    }) || any_succeeded
                        .iter()
                        .all(|candidate| complete(ledger, candidate))
                }
                DependsOn::After(_) => true,
            };
            candidates
                && step
                    .prerequisites()
                    .into_iter()
                    .all(|prerequisite| complete(ledger, prerequisite))
        }
        Node::Requirement(_) => spec.steps.iter().all(|step| complete(ledger, &step.id)),
        Node::Disposition(_) => true,
    }
}

/// Resolve a requirement's bindings, with its owning invocation's missing
/// output as one more gap.
///
/// # Errors
/// Returns the strongest gap among the owner and the bindings.
fn requirement_bindings(
    spec: &VerifierSpec,
    scope: &Scope<'_>,
    attached: &Attached,
    key: Key,
) -> Result<Bindings, Missing> {
    let owner = key
        .0
        .and_then(|index| match scope.step(&spec.steps[index].id) {
            Some(Settled::NoOutput { gap, .. }) => Some(gap.clone()),
            _ => None,
        });
    match check_bindings(scope, attached, requirement_at(spec, key)) {
        Ok(inputs) if owner.is_none() => Ok(inputs),
        Ok(_) => Err(owner.unwrap_or_else(|| Missing::failed("a binding did not resolve"))),
        Err(gap) => Err(Missing::strongest(owner.into_iter().chain([gap]))
            .unwrap_or_else(|| Missing::failed("a binding did not resolve"))),
    }
}

/// The judgment of every requirement of step `index` whose invocation
/// `done` produced no output: each takes the strongest of that gap and its
/// own binding gaps, without a call.
///
/// # Panics
/// Panics when `index` does not name a step of `spec`.
fn missing_requirements(
    spec: &VerifierSpec,
    scope: &Scope<'_>,
    attached: &Attached,
    index: usize,
    done: &StepDone,
) -> Vec<ExecutedRequirement> {
    let frame = Frame {
        own: Some(done.clone()),
        ..Frame::default()
    };
    let scope = scope.framed(frame);
    (0..spec.steps[index].requirements.len())
        .map(|at| {
            let key = (Some(index), at);
            let gap = requirement_bindings(spec, &scope, attached, key)
                .err()
                .unwrap_or_else(|| Missing::failed("the step produced no output"));
            Judged::missing(gap).report(spec, key, (None, None))
        })
        .collect()
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

/// One block the driver schedules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Node {
    /// The step at this index of `steps`, with its requirement instances.
    Step(usize),
    /// The whole-Verifier requirement at this index of `requirements`.
    Requirement(usize),
    /// The recorded disposition of the speculative step at this index.
    Disposition(usize),
}

impl Node {
    /// The ledger id this node settles.
    ///
    /// # Panics
    /// Panics when the node's index does not name a declaration of `spec`.
    fn id(self, spec: &VerifierSpec) -> &TaskId {
        match self {
            Self::Step(index) | Self::Disposition(index) => &spec.steps[index].id,
            Self::Requirement(at) => &spec.requirements[at].id,
        }
    }
}

/// A requirement's declaration: the index of its containing step, or `None`
/// for a whole-Verifier requirement, and its index in that list.
type Key = (Option<usize>, usize);

/// `finished`'s value, or its error once every other task in `running`
/// finished when the error is a capacity refusal: the execution is deferred
/// without spending attempts already dispatched, which settle and are
/// recorded instead of being dropped and counted as interrupted.
///
/// # Errors
/// Returns `finished`'s error.
async fn settled<K: PartialEq, O>(
    finished: Result<O, EngineError>,
    running: &mut Tasks<K, O, EngineError>,
) -> Result<O, EngineError> {
    if matches!(finished, Err(EngineError::Busy(_))) {
        running.drain(EngineError::Join).await;
    }
    finished
}

/// The block and instance `producer` is an attempt of.
fn instance_key(producer: &Producer) -> (TaskId, Instance) {
    (
        producer.step_id.clone(),
        (producer.item_index, producer.iteration_index),
    )
}

/// The attempt `attempt` of `task`'s invocation `instance`.
fn producer(task: &TaskId, (item_index, iteration_index): Instance, attempt: u32) -> Producer {
    Producer {
        step_id: task.clone(),
        item_index,
        iteration_index,
        attempt,
    }
}

/// The original slots a settled invocation's output derives from.
fn sources(state: &Settled) -> BTreeSet<SlotName> {
    match state {
        Settled::Succeeded { sources, .. } => sources.clone(),
        Settled::NoOutput { .. } => BTreeSet::new(),
    }
}

/// The reported result of a settled invocation with its output, which
/// [`StepDone::state`] alone holds while the run lasts.
fn reported(done: &StepDone) -> StepResult {
    let mut result = done.result.clone();
    if let Settled::Succeeded { output, .. } = &done.state {
        result.output = Some(Value::clone(output));
    }
    result
}

/// The settled invocation `instance` of a step from its reported `result`,
/// whose output moves into the shared state that bindings select from.
fn own(mut result: StepResult, sources: BTreeSet<SlotName>, instance: Instance) -> StepDone {
    let state = match (result.status, result.output.take()) {
        (StepStatus::Succeeded, Some(output)) => Settled::Succeeded {
            output: Arc::new(output),
            sources,
        },
        (status, _) => Settled::NoOutput {
            status,
            gap: Missing::failed(result.reason.clone().unwrap_or_default()),
        },
    };
    let invocations = attempt_records(
        instance.0,
        instance.1,
        result.attempts,
        result.status,
        result.reason.as_deref(),
    )
    .collect();
    let member = matches!(state, Settled::Succeeded { .. })
        .then(|| producer(&result.id, instance, result.attempts));
    StepDone {
        result,
        state,
        requirements: Vec::new(),
        invocations,
        member,
        speculation: Speculation::None,
    }
}

/// A step that did not run, skipped with `gap` and no requirement
/// judgments yet.
fn unrun(step: &Step, gap: Missing, speculation: Speculation) -> StepDone {
    StepDone {
        result: StepResult {
            id: step.id.clone(),
            status: StepStatus::Skipped,
            output: None,
            attempts: 0,
            duration_ms: 0,
            reason: Some(gap.reason.clone()),
            provenance: None,
            tool_calls: Vec::new(),
        },
        state: Settled::NoOutput {
            status: StepStatus::Skipped,
            gap,
        },
        requirements: Vec::new(),
        invocations: Vec::new(),
        member: None,
        speculation,
    }
}

/// One member of a step: a fanout item, or the step's only member.
struct Member {
    /// The zero-based item, `None` without fanout.
    item: Option<u32>,
    /// The final invocation: the last iteration, with its outcome.
    last: StepDone,
    /// Every requirement instance across its iterations.
    requirements: Vec<ExecutedRequirement>,
    /// Every invocation attempt across its iterations.
    invocations: Vec<ProducerInvocation>,
}

/// The settled step from its `members` in item order.
///
/// Without fanout the step is its one member's final invocation. With
/// fanout it succeeds only when every member did, its output the members'
/// outputs in item order (`[]` for no member); a failed member fails the
/// step with that member's reason, and no partial aggregate is published.
fn aggregate(step: &Step, mut members: Vec<Member>, fanout: bool, elapsed: Duration) -> StepDone {
    let duration_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX);
    let requirements = members
        .iter()
        .flat_map(|member| member.requirements.iter().cloned())
        .collect();
    let invocations: Vec<ProducerInvocation> = members
        .iter()
        .flat_map(|member| member.invocations.iter().cloned())
        .collect();
    let attempts = u32::try_from(invocations.len()).unwrap_or(u32::MAX);
    if !fanout
        && members.len() == 1
        && let Some(member) = members.pop()
    {
        let mut done = member.last;
        done.result.attempts = attempts;
        done.result.duration_ms = duration_ms;
        done.requirements = requirements;
        done.invocations = invocations;
        return done;
    }
    let failed = members.iter().find_map(|member| match &member.last.state {
        Settled::NoOutput { .. } => Some(format!(
            "item {}: {}",
            member.item.unwrap_or_default(),
            member
                .last
                .result
                .reason
                .as_deref()
                .unwrap_or("the item failed")
        )),
        Settled::Succeeded { .. } => None,
    });
    let tool_calls = members
        .iter()
        .flat_map(|member| member.last.result.tool_calls.iter().cloned())
        .collect();
    let (status, output, state) = if let Some(reason) = failed.clone() {
        (
            StepStatus::Failed,
            None,
            Settled::NoOutput {
                status: StepStatus::Failed,
                gap: Missing::failed(reason),
            },
        )
    } else {
        let output = Value::Array(
            members
                .iter()
                .filter_map(|member| match &member.last.state {
                    Settled::Succeeded { output, .. } => Some(Value::clone(output)),
                    Settled::NoOutput { .. } => None,
                })
                .collect(),
        );
        let sources = members
            .iter()
            .flat_map(|member| sources(&member.last.state))
            .collect();
        (
            StepStatus::Succeeded,
            None,
            Settled::Succeeded {
                output: Arc::new(output),
                sources,
            },
        )
    };
    StepDone {
        result: StepResult {
            id: step.id.clone(),
            status,
            output,
            attempts,
            duration_ms,
            reason: failed,
            provenance: None,
            tool_calls,
        },
        state,
        requirements,
        invocations,
        member: None,
        speculation: Speculation::None,
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
    /// The settled invocation `instance` of `step`.
    fn settle(self, step: &Step, sources: BTreeSet<SlotName>, instance: Instance) -> StepDone {
        let duration_ms = u64::try_from(self.duration.as_millis()).unwrap_or(u64::MAX);
        let result = match self.result {
            Ok(output) => StepResult {
                id: step.id.clone(),
                status: StepStatus::Succeeded,
                output: Some(output.output),
                attempts: self.attempts,
                duration_ms,
                reason: None,
                provenance: output.provenance,
                tool_calls: output.tool_calls,
            },
            Err(reason) => StepResult {
                id: step.id.clone(),
                status: StepStatus::Failed,
                output: None,
                attempts: self.attempts,
                duration_ms,
                reason: Some(reason),
                provenance: None,
                tool_calls: Vec::new(),
            },
        };
        own(result, sources, instance)
    }
}

/// One requirement's judgment before capture and attribution.
struct Judged {
    /// The outcome.
    outcome: RequirementOutcome,
    /// Why it holds.
    reason: String,
    /// The compared actual value, already captured under the Verifier's
    /// context-capture mode.
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
    /// The reported result of the requirement at `key` for its owner's
    /// invocation `instance`.
    ///
    /// # Panics
    /// Panics when `key` does not name a requirement of `spec`.
    fn report(
        self,
        spec: &VerifierSpec,
        key: Key,
        (item_index, iteration_index): Instance,
    ) -> ExecutedRequirement {
        let requirement = requirement_at(spec, key);
        RequirementDetails {
            result: RequirementResult {
                id: requirement.id.clone(),
                step: key.0.map(|index| spec.steps[index].id.clone()),
                item_index,
                iteration_index,
                statement: requirement.statement.clone(),
                outcome: self.outcome,
                reason: self.reason,
                actual: self.actual,
                citations: self.citations,
                data: self.data,
                detail: self.detail,
            },
            check: requirement.check.clone(),
            provenance: self.provenance,
        }
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
    /// values cannot be compared. The borrowed actual value is captured
    /// under `capture`, so only `full` copies it.
    ///
    /// # Errors
    /// Returns the capture failure of an actual value that cannot be hashed.
    fn compared(
        compared: Result<Compared<'_>, ExecError>,
        capture: ContextCapture,
    ) -> Result<Self, ExecError> {
        match compared {
            Ok((passed, actual, message)) => {
                let outcome = if passed {
                    RequirementOutcome::Passed
                } else {
                    RequirementOutcome::Failed
                };
                Ok(Self {
                    actual: capture_actual(actual, capture)?,
                    ..Self::bare(
                        outcome,
                        message.unwrap_or_else(|| "the comparison held".to_owned()),
                    )
                })
            }
            Err(error) => Ok(Self::inconclusive(error.to_string())),
        }
    }

    /// An executor's judgment, unchanged except that its actual value is
    /// captured under `capture`.
    ///
    /// # Errors
    /// Returns the capture failure of an actual value that cannot be hashed.
    fn checked(output: CheckOutput, capture: ContextCapture) -> Result<Self, ExecError> {
        let outcome = match output.verdict.outcome {
            VerificationVerdict::Passed => RequirementOutcome::Passed,
            VerificationVerdict::Failed => RequirementOutcome::Failed,
            VerificationVerdict::Inconclusive => RequirementOutcome::Inconclusive,
        };
        Ok(Self {
            outcome,
            reason: output.verdict.reason,
            actual: capture_actual(output.actual.map(Cow::Owned), capture)?,
            citations: output.verdict.citations,
            data: output.verdict.data,
            detail: output.detail,
            provenance: output.provenance,
        })
    }

    /// An Agent or code judgment: citations outside the call's original
    /// `sources` are dropped, and a pass left without a citation is
    /// inconclusive.
    ///
    /// # Errors
    /// Returns the capture failure of an actual value that cannot be hashed.
    fn cited(
        output: CheckOutput,
        sources: &BTreeSet<SlotName>,
        capture: ContextCapture,
    ) -> Result<Self, ExecError> {
        let mut judged = Self::checked(output, capture)?;
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
        Ok(judged)
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

/// The JSON a comparison reads: a selected JSON value itself, borrowed, and
/// evidence as its declaration's JSON form. Publications never reach a
/// comparison, since validation admits them only as workspace sources.
fn json(input: &CallInput) -> Cow<'_, Value> {
    match &input.value {
        CallValue::Json(value) => Cow::Borrowed(&**value),
        CallValue::Evidence(declaration) => {
            Cow::Owned(serde_json::to_value(declaration).unwrap_or(Value::Null))
        }
        CallValue::Publication { .. } | CallValue::Publications { .. } => Cow::Owned(Value::Null),
    }
}

/// The bounded cause a block error carries.
fn reason(error: &BlockError) -> String {
    match error {
        BlockError::Retryable { reason }
        | BlockError::Terminal { reason }
        | BlockError::Busy { reason }
        | BlockError::Unavailable { reason } => reason.clone(),
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
