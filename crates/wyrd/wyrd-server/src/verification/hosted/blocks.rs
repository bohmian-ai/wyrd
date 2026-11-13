//! The process-wide composition of every hosted Verifier block.
//!
//! [`HostedBlocks`] owns the capacity of one server process — a direct and a
//! queued lane, each with its own Python sandbox pool, workspace permits, and
//! model-call permits, plus the direct execution permits and the Deployment
//! limits — and builds one [`VerificationEngine`] per execution over the lane
//! of its [`ExecutionMode`]. Neither lane borrows the other's free capacity.
//! Each engine's executors are bound to that execution's accepted caller and
//! pinned evidence, so interleaved executions never share authority,
//! evidence, or a workspace. Direct verification and the queued runner both
//! build their engine here, which keeps one execution path for every caller.
//!
//! The engine records every block's run interval on the execution's
//! [`BlockClock`], so `wyrd_verification_verifier_seconds` holds the
//! Verifier's own work and `wyrd_verification_overhead_seconds` the rest.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use monty_pool::PoolError;
use skald_spec::PreparedSchemas;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;
use vala_verify::blocks::{
    ActionExecutor, ActionOutput, AgentCardResolver, AgentExecutor, AgentToolset, BlockCall,
    BlockClock, BlockError, CallInput, CheckExecutor, CheckOutput, DriftChecks, EvidenceReader,
    PromptCardResolver, ToolSession, VerificationEngine,
};
use vala_verify::{TraceSource, TraceUnavailable};
use wyrd_spec::card::agent::{BuiltinTool, ExecutionRoute};
use wyrd_spec::card::verifier::{Action, Check};
use wyrd_spec::ids::CardUid;
use wyrd_spec::vala::ids::TraceId;
use wyrd_spec::vala::trace::SpanRecord;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::Producer;

use super::evidence::TenantEvidence;
use super::registry::TenantRegistry;
use super::sandbox::{CodeExecutor, Sandbox};
use super::tools::HostedTools;
use super::traces::CallerTraces;
use super::workspace::Workspaces;
use super::{HostedLimits, LaneLimits};
use crate::components::auth::Caller;
use crate::components::gateway::{GatewayCallContext, ServerWyrdGatewayCaller};
use crate::state::AppState;
use crate::verification::state::RunState;
use crate::verification::telemetry::ExecutionMode;

/// Most steps or checks one execution runs at once.
// ponytail: fixed per-execution fan-out; the sandbox and workspace pools
// bound process capacity, so make this a Deployment limit only if needed.
const STEP_CONCURRENCY: NonZeroUsize = NonZeroUsize::new(4).expect("4 is non-zero");

/// The hosted block capacity of one server process.
pub struct HostedBlocks {
    /// Capacity reserved for direct `verify` executions.
    direct: Lane,
    /// Capacity reserved for queued executions and their continuations.
    queued: Lane,
    /// One permit per running direct execution.
    direct_executions: Arc<Semaphore>,
    /// The Deployment limits every block enforces.
    limits: HostedLimits,
    /// Agent response and Verdict schema preparation every execution reuses;
    /// it holds schemas only, never a caller's evidence or authority.
    schemas: Arc<PreparedSchemas>,
}

/// The block capacity one execution lane owns alone.
struct Lane {
    /// The lane's Python sandbox pool.
    sandbox: Arc<Sandbox>,
    /// The lane's workspace permits.
    workspaces: Arc<Workspaces>,
    /// The lane's model-call permits.
    model_calls: Arc<Semaphore>,
}

impl Lane {
    /// Start the lane sized by `lane` under `limits`.
    ///
    /// # Errors
    /// Returns [`Sandbox::start`]'s [`PoolError`].
    async fn start(
        worker: PathBuf,
        limits: HostedLimits,
        lane: LaneLimits,
    ) -> Result<Self, PoolError> {
        Ok(Self {
            sandbox: Arc::new(Sandbox::start(worker, limits, lane).await?),
            workspaces: Arc::new(Workspaces::new(limits, lane.workspaces)),
            model_calls: Arc::new(Semaphore::new(lane.model_calls)),
        })
    }
}

/// One lane admission a test can saturate with
/// [`HostedBlocks::hold_for_test`].
#[cfg(feature = "test-support")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneAdmission {
    /// Workspace permits.
    Workspaces,
    /// Python sandbox admission.
    Python,
    /// Model-call permits.
    ModelCalls,
}

/// The per-execution inputs an engine is bound to.
pub struct Execution<'a> {
    /// The accepted authority every Card, evidence, query, tool, and model
    /// call is authorized and audited as.
    pub caller: &'a Caller,
    /// The executing Verifier, whose `verifier:run` authorizes loading its
    /// Agent and Prompt definitions.
    pub verifier: &'a CardUid,
    /// Per-requirement or per-step Agent route overrides.
    pub routes: &'a BTreeMap<TaskId, ExecutionRoute>,
    /// The Card and application Run model calls are attributed to.
    pub subject: GatewayCallContext,
    /// Whether a not-yet-landed trace stops the attempt so the queue can
    /// retry it ([`VerificationEngine::with_unlanded_traces_deferred`]),
    /// rather than judging the trace check inconclusive.
    pub defer_unlanded_traces: bool,
    /// The resolved drift evidence of a scheduled window.
    pub drift: DriftChecks,
    /// The execution's block clock (`ExecutionTelemetry::clock`), on which
    /// the engine records every block's run interval.
    pub clock: BlockClock,
    /// The execution state whose accepted publications workspaces mount;
    /// `None` for an engine bound to no execution.
    pub execution: Option<Arc<RunState>>,
    /// The accepted execution mode, which selects the lane every Python
    /// operation, workspace, and model call takes capacity from.
    pub mode: ExecutionMode,
}

impl HostedBlocks {
    /// Start both lanes' sandbox pools over the Monty worker at `worker`
    /// under `limits`, prewarming `limits.monty_min_workers` in each.
    ///
    /// # Errors
    /// Returns [`Sandbox::start`]'s [`PoolError`] when the limits cannot hold
    /// the lanes' workers or a prewarmed worker cannot start.
    pub async fn start(worker: PathBuf, limits: HostedLimits) -> Result<Self, PoolError> {
        Ok(Self {
            direct: Lane::start(worker.clone(), limits, limits.direct).await?,
            queued: Lane::start(worker, limits, limits.queued).await?,
            direct_executions: Arc::new(Semaphore::new(limits.direct_executions)),
            limits,
            schemas: Arc::default(),
        })
    }

    /// Settle both lanes' admitted Python operations, refuse new ones, and
    /// reap their workers by `deadline`; see [`Sandbox::shutdown`]. Returns
    /// whether everything settled.
    pub async fn shutdown(&self, deadline: tokio::time::Instant) -> bool {
        let (direct, queued) = tokio::join!(
            self.direct.sandbox.shutdown(deadline),
            self.queued.sandbox.shutdown(deadline)
        );
        direct && queued
    }

    /// One direct execution permit, held for the execution's lifetime, or
    /// `None` when every direct execution permit is taken. Admission never
    /// waits and never takes queued capacity.
    #[must_use]
    pub fn admit_direct(&self) -> Option<OwnedSemaphorePermit> {
        Arc::clone(&self.direct_executions).try_acquire_owned().ok()
    }

    /// The Python sandbox pool of `mode`'s lane, so a test can hold one of
    /// its admissions while an execution runs.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub const fn sandbox(&self, mode: ExecutionMode) -> &Arc<Sandbox> {
        &self.lane(mode).sandbox
    }

    /// Every free permit of `admission` in `mode`'s lane, held until the
    /// returned permit drops, so a test can saturate one lane resource.
    ///
    /// # Panics
    /// Panics when the admission is closed or taken concurrently.
    #[cfg(feature = "test-support")]
    pub fn hold_for_test(
        &self,
        mode: ExecutionMode,
        admission: LaneAdmission,
    ) -> OwnedSemaphorePermit {
        let lane = self.lane(mode);
        match admission {
            LaneAdmission::Workspaces => lane.workspaces.hold_for_test(),
            LaneAdmission::Python => lane.sandbox.hold_for_test(),
            LaneAdmission::ModelCalls => super::hold_all(&lane.model_calls),
        }
    }

    /// Every free direct execution permit, held until the returned permit
    /// drops.
    ///
    /// # Panics
    /// Panics when a permit is taken concurrently.
    #[cfg(feature = "test-support")]
    pub fn hold_direct_executions_for_test(&self) -> OwnedSemaphorePermit {
        super::hold_all(&self.direct_executions)
    }

    /// The lane `mode` executes on.
    const fn lane(&self, mode: ExecutionMode) -> &Lane {
        match mode {
            ExecutionMode::Direct => &self.direct,
            ExecutionMode::Queued => &self.queued,
        }
    }

    /// The Deployment limits in force.
    #[must_use]
    pub const fn limits(&self) -> &HostedLimits {
        &self.limits
    }

    /// Build the engine of one execution over `state`, on the lane of
    /// `execution.mode`.
    ///
    /// Setup binds the execution once: its evidence reader, which admits the
    /// evidence the engine run pins before any block runs, its code executor,
    /// and an [`ExecutionBinding`] holding the accepted caller, Verifier,
    /// attribution, route overrides, and lane capacity. The Agent executor,
    /// its registry, built-in tools, gateway caller, and the trace reader are
    /// built from that binding only when the execution first runs an Agent
    /// block, opens a tool, or reads a trace, so a Verifier that never does
    /// never builds them. Every one of them is bound to `execution.caller`:
    /// Agent and Prompt Cards resolve through the caller's audited
    /// `verifier:run` on `execution.verifier`, and every model call holds one
    /// of the lane's model-call permits while it goes through the in-process
    /// gateway as that caller, attributed to `execution.subject`. Every
    /// block's run interval is recorded on `execution.clock`.
    pub fn engine(&self, state: &AppState, execution: Execution<'_>) -> VerificationEngine {
        let caller = execution.caller;
        let lane = self.lane(execution.mode);
        let evidence = TenantEvidence::new(state.clone(), caller.clone(), self.limits);
        let evidence = Arc::new(match &execution.execution {
            Some(state) => evidence.with_execution(Arc::clone(state)),
            None => evidence,
        });
        let binding = Arc::new(ExecutionBinding {
            state: state.clone(),
            caller: caller.clone(),
            verifier: execution.verifier.clone(),
            subject: execution.subject,
            routes: execution.routes.clone(),
            execution: execution.execution,
            mode: execution.mode,
            evidence: Arc::clone(&evidence),
            workspaces: Arc::clone(&lane.workspaces),
            sandbox: Arc::clone(&lane.sandbox),
            model_calls: Arc::clone(&lane.model_calls),
            limits: self.limits,
            schemas: Arc::clone(&self.schemas),
            tools: OnceLock::new(),
            traces: OnceLock::new(),
        });
        let blocks = Arc::new(Blocks {
            code: CodeExecutor::new(Arc::clone(&lane.sandbox), Arc::clone(&evidence)),
            binding: Arc::clone(&binding),
            agent: OnceLock::new(),
        });
        let engine = VerificationEngine::new(
            Arc::clone(&blocks) as Arc<dyn ActionExecutor>,
            blocks,
            binding,
            STEP_CONCURRENCY,
        )
        .with_drift(execution.drift)
        .with_evidence(evidence)
        .with_clock(execution.clock);
        if execution.defer_unlanded_traces {
            engine.with_unlanded_traces_deferred()
        } else {
            engine
        }
    }
}

/// One execution's accepted authority and lane capacity, from which its
/// Agent executor, built-in tools, and trace reader are built on first use.
///
/// It is bound to exactly one execution and never shared with another, so
/// interleaved executions never exchange authority, evidence, publications,
/// or Python budgets. It owns nothing that owns it: [`Blocks`] holds it, and
/// the Agent executor holds it as its toolset.
struct ExecutionBinding {
    /// Server state every Card, evidence, query, tool, and model call uses.
    state: AppState,
    /// The accepted caller every one of them is authorized and audited as.
    caller: Caller,
    /// The executing Verifier, whose `verifier:run` authorizes loading its
    /// Agent and Prompt definitions.
    verifier: CardUid,
    /// The Card and application Run model calls are attributed to.
    subject: GatewayCallContext,
    /// Per-requirement or per-step Agent route overrides.
    routes: BTreeMap<TaskId, ExecutionRoute>,
    /// The execution state `publish` accepts into; `None` for an engine
    /// bound to no execution.
    execution: Option<Arc<RunState>>,
    /// The accepted execution mode, which names the lane of model calls.
    mode: ExecutionMode,
    /// The execution's pinned evidence.
    evidence: Arc<TenantEvidence>,
    /// The lane's workspace permits.
    workspaces: Arc<Workspaces>,
    /// The lane's Python sandbox pool.
    sandbox: Arc<Sandbox>,
    /// The lane's model-call permits.
    model_calls: Arc<Semaphore>,
    /// The Deployment limits in force.
    limits: HostedLimits,
    /// The process's shared schema preparation.
    schemas: Arc<PreparedSchemas>,
    /// The execution's built-in tools, built when an Agent first opens a
    /// tool session; they keep each action's Python budget across attempts.
    tools: OnceLock<HostedTools>,
    /// The execution's trace reader, built on its first trace read.
    traces: OnceLock<CallerTraces>,
}

impl ExecutionBinding {
    /// The Agent executor of `binding`'s execution, resolving Agent and
    /// Prompt Cards as its caller and calling models through the gateway on
    /// its lane, with `binding` as its toolset.
    fn agent(binding: &Arc<Self>) -> AgentExecutor {
        let registry = Arc::new(TenantRegistry::new(
            binding.state.clone(),
            Some(binding.caller.clone()),
            binding.verifier.clone(),
        ));
        AgentExecutor::new(
            Arc::default(),
            Arc::clone(&registry) as Arc<dyn AgentCardResolver>,
            registry as Arc<dyn PromptCardResolver>,
        )
        .with_evidence(Arc::clone(&binding.evidence) as Arc<dyn EvidenceReader>)
        .with_toolset(Arc::clone(binding) as Arc<dyn AgentToolset>)
        .with_wyrd_gateway(Arc::new(
            ServerWyrdGatewayCaller::new(binding.state.clone(), binding.caller.clone())
                .attributed(Some(binding.subject.clone()))
                .admitted(binding.mode.as_str(), Arc::clone(&binding.model_calls)),
        ))
        .with_route_overrides(binding.routes.clone())
        .with_prepared_schemas(Arc::clone(&binding.schemas))
    }

    /// The execution's built-in tools, built on first use.
    fn tools(&self) -> &HostedTools {
        self.tools.get_or_init(|| {
            let tools = HostedTools::new(
                self.state.clone(),
                self.caller.clone(),
                Arc::clone(&self.evidence),
                Arc::clone(&self.workspaces),
                Arc::clone(&self.sandbox),
                self.limits,
            );
            match &self.execution {
                Some(run) => tools.with_execution(Arc::clone(run)),
                None => tools,
            }
        })
    }
}

#[async_trait]
impl AgentToolset for ExecutionBinding {
    /// Open the tool session through the execution's built-in tools; see
    /// [`HostedTools`].
    ///
    /// # Errors
    /// Returns the [`HostedTools`] refusal.
    async fn open(
        &self,
        id: &TaskId,
        workspace: &BTreeMap<String, CallInput>,
        deadline: Instant,
        tools: &[BuiltinTool],
        producer: Option<&Producer>,
    ) -> Result<ToolSession, BlockError> {
        self.tools()
            .open(id, workspace, deadline, tools, producer)
            .await
    }
}

#[async_trait]
impl TraceSource for ExecutionBinding {
    /// Read `trace_id` as the execution's caller; see [`CallerTraces`].
    ///
    /// # Errors
    /// Returns the [`CallerTraces`] [`TraceUnavailable`].
    async fn fetch(
        &self,
        trace_id: TraceId,
        deadline: Duration,
    ) -> Result<Arc<Vec<SpanRecord>>, TraceUnavailable> {
        self.traces
            .get_or_init(|| CallerTraces::new(self.state.clone(), self.caller.clone()))
            .fetch(trace_id, deadline)
            .await
    }
}

/// One execution's block executors, routed by block kind.
struct Blocks {
    /// The execution's binding; runs `extract` actions over its evidence.
    binding: Arc<ExecutionBinding>,
    /// Runs `code` actions and checks in the sandbox.
    code: CodeExecutor,
    /// Runs `agent` actions and checks; built on the first one.
    agent: OnceLock<AgentExecutor>,
}

impl Blocks {
    /// The execution's Agent executor, built once on first use.
    fn agent(&self) -> &AgentExecutor {
        self.agent
            .get_or_init(|| ExecutionBinding::agent(&self.binding))
    }
}

#[async_trait]
impl ActionExecutor for Blocks {
    /// Route `call` to the executor of its action kind.
    ///
    /// # Errors
    /// Returns the routed executor's [`BlockError`].
    async fn execute(&self, call: BlockCall<'_, Action>) -> Result<ActionOutput, BlockError> {
        match call.block {
            Action::Extract { .. } => self.binding.evidence.execute(call).await,
            Action::Agent(_) => ActionExecutor::execute(self.agent(), call).await,
            Action::Code(_) => ActionExecutor::execute(&self.code, call).await,
        }
    }
}

#[async_trait]
impl CheckExecutor for Blocks {
    /// Route a `code` check to the sandbox and every other executor-run
    /// check (`agent`) to the Agent executor; the engine itself judges
    /// `assert`, `trace`, and `drift`.
    ///
    /// # Errors
    /// Returns the routed executor's [`BlockError`].
    async fn execute(&self, call: BlockCall<'_, Check>) -> Result<CheckOutput, BlockError> {
        match call.block {
            Check::Code(_) => CheckExecutor::execute(&self.code, call).await,
            _ => CheckExecutor::execute(self.agent(), call).await,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Proof that hosted executor time is Verifier time, not overhead.

    use std::time::Duration;

    use super::*;
    use crate::verification::telemetry::{ExecutionMode, ExecutionTelemetry, Phase};

    /// A code action that takes 50 ms of external time.
    struct Slow;

    #[async_trait]
    impl ActionExecutor for Slow {
        /// Sleep 50 ms, then output `{}`.
        ///
        /// # Errors
        /// Never fails.
        async fn execute(&self, _call: BlockCall<'_, Action>) -> Result<ActionOutput, BlockError> {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Ok(ActionOutput {
                output: serde_json::json!({}),
                provenance: None,
                tool_calls: Vec::new(),
            })
        }
    }

    #[async_trait]
    impl CheckExecutor for Slow {
        /// Never called: the Verifier has no executor check.
        ///
        /// # Errors
        /// Always refuses.
        async fn execute(&self, _call: BlockCall<'_, Check>) -> Result<CheckOutput, BlockError> {
            Err(BlockError::Terminal {
                reason: "unexpected check".to_owned(),
            })
        }
    }

    /// An engine on the execution's clock spends a 50 ms action as Verifier
    /// time: the engine phase and Verifier time hold it and the overhead
    /// does not.
    ///
    /// # Panics
    /// Panics when the run fails or the overhead absorbs the action.
    #[tokio::test]
    async fn executor_time_is_verifier_time_not_overhead() {
        let recorder = metrics_exporter_prometheus::PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        let _guard = metrics::set_default_local_recorder(&recorder);
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Direct, None);
        let blocks = Arc::new(Slow);
        let engine = VerificationEngine::new(
            Arc::clone(&blocks) as Arc<dyn ActionExecutor>,
            blocks,
            Arc::new(vala_verify::InMemoryTraceSource::new()),
            STEP_CONCURRENCY,
        )
        .with_clock(telemetry.clock());
        let spec: wyrd_spec::card::verifier::VerifierSpec = serde_json::from_value(serde_json::json!({
            "steps": [{"id": "work", "action": {"kind": "code", "entrypoint": "run", "source": {"inline": "def run(): pass"}, "inputs": {}}}],
            "requirements": [{"id": "done", "statement": "Work ran", "check": {"kind": "assert", "path": "steps.work.status", "operator": "equals", "expected": "succeeded"}}]
        }))
        .expect("Verifier");
        telemetry
            .phase(
                Phase::Engine,
                engine.run(
                    Arc::new(spec),
                    BTreeMap::new(),
                    tokio::time::Instant::now() + Duration::from_secs(5),
                ),
            )
            .await
            .expect("outcome");
        telemetry.finish("completed", false);
        let text = handle.render();
        let sum = |series: &str| -> f64 {
            text.lines()
                .find(|line| line.starts_with(series))
                .and_then(|line| line.rsplit(' ').next())
                .and_then(|value| value.parse().ok())
                .unwrap_or(f64::NAN)
        };
        let engine =
            sum(r#"wyrd_verification_phase_duration_seconds_sum{mode="direct",phase="engine"}"#);
        let verifier =
            sum(r#"wyrd_verification_verifier_seconds_sum{mode="direct",outcome="completed"}"#);
        let overhead =
            sum(r#"wyrd_verification_overhead_seconds_sum{mode="direct",outcome="completed"}"#);
        assert!(engine >= 0.05, "{text}");
        assert!(verifier >= 0.05, "{text}");
        assert!(overhead < 0.025, "{text}");
    }

    /// Two concurrent 50 ms actions of one real engine run land in the
    /// benchmark histograms as one `verification.direct` observation whose
    /// excluded wait counts their overlap once, about 50 ms rather than
    /// 100, and whose residual is the execution less that union; its
    /// engine phase lands as one `verification.direct.engine` part holding
    /// the run, and the time outside every phase as one
    /// `verification.direct.outside` part; no production duration histogram
    /// counts the execution a second time.
    ///
    /// # Panics
    /// Panics when the run fails, the histogram observation, wait,
    /// residual, or parts are wrong, or a production histogram repeats it.
    #[tokio::test]
    async fn histogram_overhead_excludes_real_overlapping_blocks() {
        use wyrd_benchmark as timing;

        timing::install().expect("the benchmark recorder installs");
        let before = timing::snapshot();
        let telemetry = ExecutionTelemetry::start(ExecutionMode::Direct, None);
        let blocks = Arc::new(Slow);
        let engine = VerificationEngine::new(
            Arc::clone(&blocks) as Arc<dyn ActionExecutor>,
            blocks,
            Arc::new(vala_verify::InMemoryTraceSource::new()),
            STEP_CONCURRENCY,
        )
        .with_clock(telemetry.clock());
        let action = serde_json::json!({"kind": "code", "entrypoint": "run", "source": {"inline": "def run(): pass"}, "inputs": {}});
        let spec: wyrd_spec::card::verifier::VerifierSpec = serde_json::from_value(serde_json::json!({
            "steps": [{"id": "left", "action": action}, {"id": "right", "action": action}],
            "requirements": [{"id": "done", "statement": "Work ran", "check": {"kind": "assert", "path": "steps.left.status", "operator": "equals", "expected": "succeeded"}}]
        }))
        .expect("Verifier");
        telemetry
            .phase(
                Phase::Engine,
                engine.run(
                    Arc::new(spec),
                    BTreeMap::new(),
                    tokio::time::Instant::now() + Duration::from_secs(5),
                ),
            )
            .await
            .expect("outcome");
        telemetry.finish("completed", false);
        let interval = timing::snapshot().since(&before).expect("one process");
        let boundaries: Vec<(&str, &str, u64)> = interval
            .series
            .iter()
            .map(|series| {
                (
                    series.boundary.as_str(),
                    series.outcome.as_str(),
                    series.count,
                )
            })
            .collect();
        assert_eq!(
            boundaries,
            [
                ("verification.direct", "completed", 1),
                ("verification.direct.engine", "completed", 1),
                ("verification.direct.outside", "completed", 1),
            ]
        );
        let series = &interval.series[0];
        assert!(series.problems().is_empty(), "{:?}", series.problems());
        let ms = |distribution: Option<&timing::Distribution>| {
            let bucket = distribution
                .and_then(timing::Distribution::maximum)
                .expect("one observation");
            (bucket.lowest / 1_000_000, bucket.highest / 1_000_000)
        };
        let (_, waited) = ms(series.excluded.as_ref());
        let (lowest, _) = ms(series.excluded.as_ref());
        assert!(
            lowest >= 50 && waited < 90,
            "overlap counted once: {lowest}..{waited} ms"
        );
        let (_, residual) = ms(series.residual.as_ref());
        assert!(
            residual < 25,
            "the residual is the execution less the union: {residual} ms"
        );
        let (elapsed, _) = ms(Some(&series.elapsed));
        assert!(elapsed >= lowest, "the execution holds its blocks");
        let (phase, _) = ms(Some(&interval.series[1].elapsed));
        assert!(phase >= lowest, "the engine phase holds the run");
        let (_, outside) = ms(Some(&interval.series[2].elapsed));
        assert!(outside < 25, "nothing ran outside the engine: {outside} ms");
        for family in [
            crate::app::metrics::VERIFICATION_PHASE_DURATION_SECONDS,
            crate::app::metrics::VERIFICATION_VERIFIER_SECONDS,
            crate::app::metrics::VERIFICATION_OVERHEAD_SECONDS,
            crate::app::metrics::VERIFICATION_RUN_DURATION_SECONDS,
        ] {
            assert!(
                interval.histogram(family, &[]).is_err(),
                "{family} would count the timed execution twice"
            );
        }
    }
}

#[cfg(test)]
mod pg_tests {
    //! Proof that a direct execution holds one copy of its evidence through
    //! the engine and the hosted evidence reader, compares and hashes
    //! selected JSON without disposable copies, and keeps one owner of a
    //! successful step's output.

    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::Duration;

    use serde_json::{Value, json};
    use sha2::Digest as _;
    use vala_verify::InMemoryTraceSource;
    use wyrd_spec::card::verifier::VerifierSpec;
    use wyrd_spec::ids::SlotName;
    use wyrd_spec::verification::VerificationVerdict;
    use wyrd_spec::verification::execution::{
        ExecutedOutcome, InputDeclaration, RequirementOutcome,
    };

    use super::*;
    use crate::test_support::{CountingAllocator, test_caller, test_state};

    /// The payload bytes a case grows by between its two measurements.
    const GROWTH: usize = 1 << 20;

    /// A code action answering one prebuilt output, so building it is the
    /// test's work rather than the engine's.
    struct Prebuilt(Mutex<Option<Value>>);

    #[async_trait]
    impl ActionExecutor for Prebuilt {
        /// Answer the prebuilt output once, then `null`.
        ///
        /// # Errors
        /// Never fails.
        async fn execute(&self, _call: BlockCall<'_, Action>) -> Result<ActionOutput, BlockError> {
            Ok(ActionOutput {
                output: self
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .take()
                    .unwrap_or(Value::Null),
                provenance: None,
                tool_calls: Vec::new(),
            })
        }
    }

    #[async_trait]
    impl CheckExecutor for Prebuilt {
        /// Never called: the Verifier has no executor check.
        ///
        /// # Errors
        /// Always refuses.
        async fn execute(&self, _call: BlockCall<'_, Check>) -> Result<CheckOutput, BlockError> {
            Err(BlockError::Terminal {
                reason: "unexpected check".to_owned(),
            })
        }
    }

    /// The payload-sized copies `measured` makes: its allocation growth per
    /// payload byte between payloads of `GROWTH` and twice `GROWTH` bytes,
    /// so fixed bookkeeping cancels out.
    #[expect(
        clippy::cast_precision_loss,
        reason = "byte counts of a few MiB are exact in f64"
    )]
    fn copies(measured: impl Fn(usize) -> usize) -> f64 {
        let (small, large) = (measured(GROWTH), measured(2 * GROWTH));
        large.saturating_sub(small) as f64 / GROWTH as f64
    }

    /// One JSON value holding `bytes` bytes of text.
    fn payload(bytes: usize) -> Value {
        json!({"text": "x".repeat(bytes), "escaped": "\"\\\n\u{e9}"})
    }

    /// A direct execution with large inline evidence judges a whole-input
    /// assertion under hashed capture without copying the payload: the
    /// binding points at the pinned evidence, and building the engine,
    /// admitting the evidence to the hosted reader, and comparing and hashing
    /// the borrowed actual value copy nothing, while the outcome returns the
    /// original evidence and the compact-Serde digest. A successful step's
    /// large output serves its dependent requirement and the step result with
    /// one copy under full capture, the stored actual value, its output
    /// owned once.
    ///
    /// # Panics
    /// Panics when a run fails, its outcome differs, or it copies the
    /// payload more often.
    #[test]
    fn direct_execution_reuses_evidence_without_disposable_payload_copies() {
        let runtime = wyrd_runtime::runtime();
        let limits = HostedLimits {
            monty_min_workers: 0,
            ..HostedLimits::default()
        };
        let blocks = runtime
            .block_on(HostedBlocks::start(
                PathBuf::from("/nonexistent/wyrd-server"),
                limits,
            ))
            .expect("an empty pool starts");
        let state = runtime.block_on(test_state());
        let caller = test_caller([]);
        let routes = BTreeMap::new();
        let uid = CardUid::from_uuid(uuid::Uuid::now_v7()).expect("uid");
        let deadline = || tokio::time::Instant::now() + Duration::from_secs(30);
        let verifier = |spec: Value| -> Arc<VerifierSpec> {
            Arc::new(serde_json::from_value(spec).expect("Verifier"))
        };
        let passed = |outcome: &ExecutedOutcome| {
            assert_eq!(outcome.verdict, VerificationVerdict::Passed);
            assert_eq!(
                outcome.requirements[0].result.outcome,
                RequirementOutcome::Passed
            );
        };

        let slot = SlotName::new("context").expect("slot");
        let assertion = verifier(json!({
            "inputs": {"context": {"type": "json"}},
            "requirements": [{"id": "present", "statement": "The context is present", "check": {"kind": "assert", "path": "inputs.context", "operator": "is_not_null"}}],
            "context_capture": "hash"
        }));
        let evidence_copies = copies(|bytes| {
            let context = payload(bytes);
            let evidence = BTreeMap::from([(
                slot.clone(),
                InputDeclaration::Json {
                    value: context.clone(),
                },
            )]);
            let expected = evidence.clone();
            let (outcome, allocated) = CountingAllocator::measure(|| {
                let engine = blocks.engine(
                    &state,
                    Execution {
                        caller: &caller,
                        verifier: &uid,
                        routes: &routes,
                        subject: GatewayCallContext::default(),
                        defer_unlanded_traces: false,
                        drift: DriftChecks::new(None, BTreeMap::new(), BTreeMap::new()),
                        clock: BlockClock::default(),
                        execution: None,
                        mode: ExecutionMode::Direct,
                    },
                );
                runtime.block_on(engine.run(Arc::clone(&assertion), evidence, deadline()))
            });
            let outcome = outcome.expect("outcome");
            passed(&outcome);
            assert_eq!(outcome.evidence, expected, "the original evidence returns");
            let digest = sha2::Sha256::digest(serde_json::to_vec(&context).expect("bytes"));
            assert_eq!(
                outcome.requirements[0].result.actual,
                Some(Value::String(format!("{digest:x}")))
            );
            allocated
        });
        assert!(
            evidence_copies < 0.1,
            "nothing copies the evidence: {evidence_copies:.2} copies"
        );

        let dependent = verifier(json!({
            "steps": [{"id": "work", "action": {"kind": "code", "entrypoint": "run", "source": {"inline": "def run(): pass"}, "inputs": {}}}],
            "requirements": [{"id": "present", "statement": "The output is present", "check": {"kind": "assert", "path": "steps.work.output", "operator": "is_not_null"}}]
        }));
        let output_copies = copies(|bytes| {
            let output = payload(bytes);
            let action = Arc::new(Prebuilt(Mutex::new(Some(output.clone()))));
            let (outcome, allocated) = CountingAllocator::measure(|| {
                let engine = VerificationEngine::new(
                    Arc::clone(&action) as Arc<dyn ActionExecutor>,
                    action,
                    Arc::new(InMemoryTraceSource::new()),
                    STEP_CONCURRENCY,
                )
                .with_evidence(Arc::new(TenantEvidence::new(
                    state.clone(),
                    caller.clone(),
                    limits,
                )));
                runtime.block_on(engine.run(Arc::clone(&dependent), BTreeMap::new(), deadline()))
            });
            let outcome = outcome.expect("outcome");
            passed(&outcome);
            assert_eq!(outcome.steps[0].output.as_ref(), Some(&output));
            assert_eq!(
                outcome.requirements[0].result.actual.as_ref(),
                Some(&output)
            );
            allocated
        });
        assert!(
            output_copies < 1.1,
            "only the stored actual value copies the output: {output_copies:.2} copies"
        );
    }
}
