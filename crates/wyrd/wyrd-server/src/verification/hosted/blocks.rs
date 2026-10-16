//! The process-wide composition of every hosted Verifier block.
//!
//! [`HostedBlocks`] owns the capacity one server process shares — the Python
//! sandbox pool, the workspace permits, and the Deployment limits — and
//! builds one [`VerificationEngine`] per execution over them. Each engine's
//! executors are bound to that execution's accepted caller and pinned
//! evidence, so interleaved executions never share authority, evidence, or a
//! workspace. Direct verification and the queued runner both build their
//! engine here, which keeps one execution path for every caller.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use monty_pool::PoolError;
use vala_eval::blocks::{
    ActionExecutor, ActionOutput, AgentCardResolver, AgentExecutor, BlockCall, BlockError,
    CheckExecutor, CheckOutput, DriftChecks, EvidenceReader, PromptCardResolver,
    VerificationEngine,
};
use wyrd_spec::card::agent::ExecutionRoute;
use wyrd_spec::card::verifier::{Action, Check};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::SlotName;
use wyrd_spec::vala::eval::TaskId;
use wyrd_spec::verification::execution::InputDeclaration;

use super::HostedLimits;
use super::evidence::TenantEvidence;
use super::registry::TenantRegistry;
use super::sandbox::{CodeExecutor, Sandbox};
use super::tools::HostedTools;
use super::traces::CallerTraces;
use super::workspace::Workspaces;
use crate::components::auth::Caller;
use crate::components::gateway::{GatewayCallSubject, ServerWyrdGatewayCaller};
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
    pub subject: GatewayCallSubject,
    /// When the execution was accepted, anchoring trace reads.
    pub accepted_at: DateTime<Utc>,
    /// The resolved drift evidence of a scheduled window.
    pub drift: DriftChecks,
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

    /// The Deployment limits in force.
    #[must_use]
    pub const fn limits(&self) -> &HostedLimits {
        &self.limits
    }

    /// Build the engine of one execution over `state`.
    ///
    /// The evidence reader, registry, code executor, built-in tools, and
    /// gateway caller are all bound to `execution.caller`; the evidence
    /// reader also admits the pinned evidence before any block runs; Agent and Prompt
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
        let traces = CallerTraces::new(state.clone(), caller.clone(), execution.accepted_at)?;
        Ok(VerificationEngine::new(
            Arc::clone(&blocks) as Arc<dyn ActionExecutor>,
            blocks,
            Arc::new(traces),
            STEP_CONCURRENCY,
        )
        .with_drift(execution.drift)
        .with_evidence(evidence))
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
