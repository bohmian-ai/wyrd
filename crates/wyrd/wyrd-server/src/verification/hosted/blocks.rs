//! The process-wide composition of every hosted Verifier block.
//!
//! [`HostedBlocks`] owns the capacity one server process shares — the Python
//! sandbox pool, the workspace permits, and the Deployment limits — and
//! builds one [`VerificationEngine`] per execution over them. Each engine's
//! executors are bound to that execution's accepted caller and pinned
//! evidence, so interleaved executions never share authority, evidence, or a
//! workspace. Direct verification and the queued runner both build their
//! engine here, which keeps one execution path for every caller.
//!
//! The engine records every block's run interval on the execution's
//! [`BlockClock`], so `wyrd_verification_verifier_seconds` holds the
//! Verifier's own work and `wyrd_verification_overhead_seconds` the rest.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use monty_pool::PoolError;
use vala_verify::blocks::{
    ActionExecutor, ActionOutput, AgentCardResolver, AgentExecutor, BlockCall, BlockClock,
    BlockError, CheckExecutor, CheckOutput, DriftChecks, EvidenceReader, PromptCardResolver,
    VerificationEngine,
};
use wyrd_spec::card::agent::ExecutionRoute;
use wyrd_spec::card::verifier::{Action, Check};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::SlotName;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::InputDeclaration;

use super::HostedLimits;
use super::evidence::TenantEvidence;
use super::registry::TenantRegistry;
use super::sandbox::{CodeExecutor, Sandbox};
use super::tools::HostedTools;
use super::traces::CallerTraces;
use super::workspace::Workspaces;
use crate::components::auth::Caller;
use crate::components::gateway::{GatewayCallContext, ServerWyrdGatewayCaller};
use crate::state::AppState;

/// Most steps or checks one execution runs at once.
// ponytail: fixed per-execution fan-out; the sandbox and workspace pools
// bound process capacity, so make this a Deployment limit only if needed.
const STEP_CONCURRENCY: NonZeroUsize = NonZeroUsize::new(4).expect("4 is non-zero");

/// The hosted block capacity one server process shares.
pub struct HostedBlocks {
    /// The process-wide Python sandbox pool.
    sandbox: Arc<Sandbox>,
    /// The process-wide workspace permits.
    workspaces: Arc<Workspaces>,
    /// The Deployment limits every block enforces.
    limits: HostedLimits,
}

/// The per-execution inputs an engine is bound to.
pub struct Execution<'a> {
    /// The accepted authority every Card, evidence, query, tool, and model
    /// call is authorized and audited as.
    pub caller: &'a Caller,
    /// The execution's pinned evidence, the only citable sources.
    pub evidence: &'a BTreeMap<SlotName, InputDeclaration>,
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
}

impl HostedBlocks {
    /// Start the sandbox pool over the Monty worker at `worker` under
    /// `limits`, prewarming `limits.monty_min_workers`.
    ///
    /// # Errors
    /// Returns [`Sandbox::start`]'s [`PoolError`] when the limits hold no
    /// worker or a prewarmed worker cannot start.
    pub async fn start(worker: PathBuf, limits: HostedLimits) -> Result<Self, PoolError> {
        Ok(Self {
            sandbox: Arc::new(Sandbox::start(worker, limits).await?),
            workspaces: Arc::new(Workspaces::new(limits)),
            limits,
        })
    }

    /// Settle the Python sandbox's admitted operations, refuse new ones,
    /// and reap its workers by `deadline`; see [`Sandbox::shutdown`].
    /// Returns whether everything settled.
    pub async fn shutdown(&self, deadline: tokio::time::Instant) -> bool {
        self.sandbox.shutdown(deadline).await
    }

    /// The process-wide Python sandbox pool, so a test can hold one of its
    /// admissions while a queued execution runs.
    #[cfg(feature = "test-support")]
    #[must_use]
    pub const fn sandbox(&self) -> &Arc<Sandbox> {
        &self.sandbox
    }

    /// The Deployment limits in force.
    #[must_use]
    pub const fn limits(&self) -> &HostedLimits {
        &self.limits
    }

    /// Build the engine of one execution over `state`.
    ///
    /// The evidence reader, registry, code executor, built-in tools, and
    /// gateway caller are all bound to `execution.caller`; the evidence
    /// reader also admits the pinned evidence before any block runs, and
    /// every block's run interval is recorded on `execution.clock`; Agent and Prompt
    /// Cards resolve through the caller's audited `card:read`, and every
    /// model call goes through the in-process gateway as that caller,
    /// attributed to `execution.subject`.
    ///
    /// # Errors
    /// Returns the [`CallerTraces::new`] refusal when the caller cannot form
    /// a query context.
    pub fn engine(
        &self,
        state: &AppState,
        execution: Execution<'_>,
    ) -> Result<VerificationEngine, WyrdError> {
        let caller = execution.caller;
        let evidence = Arc::new(TenantEvidence::new(
            state.clone(),
            caller.clone(),
            execution.evidence.clone(),
            self.limits,
        ));
        let registry = Arc::new(TenantRegistry::new(state.clone(), Some(caller.clone())));
        let agent = AgentExecutor::new(
            Arc::default(),
            Arc::clone(&registry) as Arc<dyn AgentCardResolver>,
            registry as Arc<dyn PromptCardResolver>,
        )
        .with_evidence(Arc::clone(&evidence) as Arc<dyn EvidenceReader>)
        .with_toolset(Arc::new(HostedTools::new(
            state.clone(),
            caller.clone(),
            Arc::clone(&evidence),
            Arc::clone(&self.workspaces),
            Arc::clone(&self.sandbox),
            self.limits,
        )))
        .with_wyrd_gateway(Arc::new(
            ServerWyrdGatewayCaller::new(state.clone(), caller.clone())
                .attributed(Some(execution.subject)),
        ))
        .with_route_overrides(execution.routes.clone());
        let blocks = Arc::new(Blocks {
            code: CodeExecutor::new(Arc::clone(&self.sandbox), Arc::clone(&evidence)),
            evidence: Arc::clone(&evidence),
            agent,
        });
        let traces = CallerTraces::new(state.clone(), caller.clone())?;
        let engine = VerificationEngine::new(
            Arc::clone(&blocks) as Arc<dyn ActionExecutor>,
            blocks,
            Arc::new(traces),
            STEP_CONCURRENCY,
        )
        .with_drift(execution.drift)
        .with_evidence(evidence)
        .with_clock(execution.clock);
        Ok(if execution.defer_unlanded_traces {
            engine.with_unlanded_traces_deferred()
        } else {
            engine
        })
    }
}

/// One execution's block executors, routed by block kind.
struct Blocks {
    /// Runs `extract` actions over the pinned evidence.
    evidence: Arc<TenantEvidence>,
    /// Runs `code` actions and checks in the sandbox.
    code: CodeExecutor,
    /// Runs `agent` actions and checks.
    agent: AgentExecutor,
}

#[async_trait]
impl ActionExecutor for Blocks {
    /// Route `call` to the executor of its action kind.
    ///
    /// # Errors
    /// Returns the routed executor's [`BlockError`].
    async fn execute(&self, call: BlockCall<'_, Action>) -> Result<ActionOutput, BlockError> {
        match call.block {
            Action::Extract { .. } => self.evidence.execute(call).await,
            Action::Agent(_) => ActionExecutor::execute(&self.agent, call).await,
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
            _ => CheckExecutor::execute(&self.agent, call).await,
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
    /// `verification.direct.outside` part.
    ///
    /// # Panics
    /// Panics when the run fails or the histogram observation, wait,
    /// residual, or parts are wrong.
    #[tokio::test]
    async fn histogram_overhead_excludes_real_overlapping_blocks() {
        use wyrd_telemetry::timing;

        timing::enable();
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
    }
}
