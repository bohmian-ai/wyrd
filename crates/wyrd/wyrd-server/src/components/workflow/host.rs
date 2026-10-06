//! Create, read, and cancel accepted server Workflow runs.
//!
//! [`WorkflowRunHost`] is what the HTTP handlers call. Every operation first
//! makes and audits a fresh `workflows:run` decision for the verified caller,
//! and only then consults the run table, so no request learns whether a run
//! or key exists without that permission. A create that owns a new
//! reservation hands it to a tracked [`Preparation`], which pins the exact
//! active graph, checks it against what this server can execute, prepares
//! the Skald run, accepts it, and executes it, all independently of the
//! request that started it.

use std::collections::BTreeSet;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::Duration;

use skald_runtime::ProviderRegistry;
use skald_workflow::{
    ExternalEndpointProfile, ExternalGatewayBindings, PreparedWorkflowRun,
    Workflow as SkaldWorkflow, WorkflowExecutionDependencies, WorkflowExecutionLimits,
    WorkflowInput, WorkflowRunOptions, WyrdGatewayCaller,
};
use tokio_util::sync::CancellationToken;
use wyrd_runtime::Permission;
use wyrd_spec::AgentSpec;
use wyrd_spec::card::workflow::{CreateWorkflowRunRequest, LlmRoute, WorkflowAction, WorkflowRun};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CredentialBindingName, IdempotencyKey, WorkflowRunId};
use wyrd_spec::reference::{CardRef, InlineableRef};

use super::runs::{Admission, Reservation, RunKey, run_not_found, run_unavailable};
use super::tools::{RunTools, is_builtin};
use crate::audit;
use crate::components::auth::Caller;
use crate::components::cards::{GraphBounds, PinnedWorkflowGraph};
use crate::components::gateway::ServerWyrdGatewayCaller;
use crate::state::{AppState, registry_db_error};

/// How a create was answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Created {
    /// This request's preparation accepted a new run.
    Accepted,
    /// The key already named, or was preparing, the same request's run.
    Replayed,
}

/// The Workflow run operations of one server.
pub(crate) struct WorkflowRunHost {
    /// Server state owning the run table and every service a run calls.
    state: AppState,
}

impl WorkflowRunHost {
    /// Bind the operations to `state`.
    pub(crate) fn new(state: AppState) -> Self {
        Self { state }
    }

    /// Accept `request` under `key` for `caller`, or answer its replay.
    ///
    /// The Workflow reference and timeout are checked, then `workflows:run`
    /// is authorized and audited, then the canonical request hash decides
    /// replay, conflict, joining a preparation, or reserving a new one. A new
    /// reservation's preparation runs on tracked work, so dropping this
    /// future never cancels or strands it; the future only waits for the
    /// preparation's published outcome.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_422_RUN_REQUEST` for a reference that is not a
    /// spaced Workflow or a timeout outside `1..=max_timeout_seconds`; the
    /// permission errors of [`audit::authorize`]; the admission
    /// errors of [`super::runs::WorkflowRuns::admit`]; and every preparation
    /// failure of [`Preparation::prepare`].
    pub(crate) async fn create(
        &self,
        caller: &Caller,
        key: IdempotencyKey,
        request: CreateWorkflowRunRequest,
    ) -> Result<(Created, WorkflowRun), WyrdError> {
        PinnedWorkflowGraph::check_ref(&request.workflow)?;
        let timeout = self.timeout(request.timeout_seconds)?;
        audit::authorize(
            &self.state,
            caller,
            &Permission::workflow_run(),
            "workflow.run.create",
            &workflow_resource(&request.workflow),
        )?;
        let hash =
            blake3::hash(&serde_jcs::to_vec(&request).map_err(WyrdError::from_spec_serialization)?);
        let key = RunKey {
            tenant: caller.data_tenant_id,
            principal: caller.principal.id,
            key,
        };
        let runs = &self.state.workflows;
        let (created, mut outcome) = match runs.admit(key, hash)? {
            Admission::Replay(run) => return Ok((Created::Replayed, *run)),
            Admission::Wait(outcome) => (Created::Replayed, outcome),
            Admission::Reserved(reservation) => {
                let outcome = reservation.outcome();
                let preparation = Preparation {
                    state: self.state.clone(),
                    caller: caller.clone(),
                    request,
                    timeout,
                    reservation,
                };
                runs.spawn(preparation.run());
                (Created::Accepted, outcome)
            }
        };
        let published = outcome
            .wait_for(Option::is_some)
            .await
            .map_err(|_| run_unavailable())?
            .clone();
        published
            .unwrap_or_else(|| Err(run_unavailable()))
            .map(|run| (created, run))
    }

    /// The current snapshot of `caller`'s run `run_id`.
    ///
    /// # Errors
    /// Returns the permission errors of [`audit::authorize`], and
    /// `WYRD_WORKFLOW_404_RUN_NOT_FOUND` for a malformed id or a run the
    /// caller cannot see.
    pub(crate) async fn get(
        &self,
        caller: &Caller,
        run_id: &str,
    ) -> Result<WorkflowRun, WyrdError> {
        let run_id = self
            .authorize_run(caller, "workflow.run.read", run_id)
            .await?;
        self.state
            .workflows
            .get(caller.data_tenant_id, caller.principal.id, run_id)
    }

    /// Cancel `caller`'s run `run_id` and return the terminal snapshot that
    /// won.
    ///
    /// The cancellation is recorded before this future waits, so dropping it
    /// does not revoke the cancellation. A terminal run is returned
    /// unchanged.
    ///
    /// # Errors
    /// Returns the permission errors of [`audit::authorize`], and
    /// `WYRD_WORKFLOW_404_RUN_NOT_FOUND` for a malformed id or a run the
    /// caller cannot see.
    pub(crate) async fn cancel(
        &self,
        caller: &Caller,
        run_id: &str,
    ) -> Result<WorkflowRun, WyrdError> {
        let run_id = self
            .authorize_run(caller, "workflow.run.cancel", run_id)
            .await?;
        let mut snapshots =
            self.state
                .workflows
                .cancel(caller.data_tenant_id, caller.principal.id, run_id)?;
        let terminal = snapshots
            .wait_for(|run| run.status.is_terminal())
            .await
            .map_err(|_| run_not_found())?
            .clone();
        Ok(terminal)
    }

    /// Authorize and audit `workflows:run` for `operation` on the raw
    /// `run_id`, then parse it.
    ///
    /// The decision is made before the id is parsed, so a malformed id is
    /// indistinguishable from an unknown one.
    ///
    /// # Errors
    /// Returns the permission errors of [`audit::authorize`], and
    /// `WYRD_WORKFLOW_404_RUN_NOT_FOUND` for a malformed id.
    async fn authorize_run(
        &self,
        caller: &Caller,
        operation: &str,
        run_id: &str,
    ) -> Result<WorkflowRunId, WyrdError> {
        let parsed = run_id.parse::<WorkflowRunId>();
        let resource = match &parsed {
            Ok(run_id) => format!("workflow-run:{run_id}"),
            Err(_) => "workflow-run:-".to_owned(),
        };
        audit::authorize(
            &self.state,
            caller,
            &Permission::workflow_run(),
            operation,
            &resource,
        )?;
        parsed.map_err(|_| run_not_found())
    }

    /// The total run deadline a request asks for, or the default.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_422_RUN_REQUEST` naming `timeout_seconds` for
    /// zero or a value above the configured maximum.
    fn timeout(&self, requested: Option<u64>) -> Result<Duration, WyrdError> {
        let config = self.state.workflows.config();
        let seconds = requested.unwrap_or(config.default_timeout_seconds);
        if seconds == 0 || seconds > config.max_timeout_seconds {
            return Err(WyrdError::WorkflowRunRequest {
                message: format!(
                    "timeout_seconds must be between 1 and {}",
                    config.max_timeout_seconds
                ),
                details: serde_json::json!({
                    "field": "timeout_seconds",
                    "max": config.max_timeout_seconds,
                }),
            });
        }
        Ok(Duration::from_secs(seconds))
    }
}

/// One tracked preparation and, once accepted, execution of a run.
struct Preparation {
    /// Server state the run's services come from.
    state: AppState,
    /// Authority captured from the accepted submission.
    caller: Caller,
    /// The submitted request.
    request: CreateWorkflowRunRequest,
    /// Total run deadline.
    timeout: Duration,
    /// The key and active slot this preparation owns.
    reservation: Reservation,
}

impl Preparation {
    /// Prepare, accept, execute, and terminalize the run.
    ///
    /// A preparation failure is published to every waiter and releases the
    /// reservation; cancellation by shutdown drops the reservation, which
    /// releases it the same way. After acceptance the Skald run executes
    /// under the reservation's token, each non-terminal transition replaces
    /// the stored snapshot, and the terminal snapshot is committed only after
    /// every query owner of the run has settled. With `test-support`, an
    /// armed preparation gate can hold the accepted run queued before it
    /// executes; cancellation releases it into the same terminalization.
    async fn run(self) {
        let Self {
            state,
            caller,
            request,
            timeout,
            reservation,
        } = self;
        let cancel = reservation.cancel_token().clone();
        let prepared = tokio::select! {
            biased;
            () = cancel.cancelled() => return,
            prepared = Self::prepare(&state, caller, request, timeout, &cancel) => prepared,
        };
        let (prepared, tools) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                reservation.fail(error);
                return;
            }
        };
        let Ok(accepted) = reservation.accept(prepared.snapshot().clone()) else {
            return;
        };
        #[cfg(feature = "test-support")]
        tokio::select! {
            biased;
            () = cancel.cancelled() => {}
            () = state.workflows.pass_preparation_gate_for_test() => {}
        }
        let terminal = prepared.execute(|run| accepted.observe(run)).await;
        tools.drain().await;
        accepted.finish(terminal);
    }

    /// Pin the graph, check it against this server, and prepare the run.
    ///
    /// Reads the exact active graph in one tenant transaction; refuses
    /// `native` routes and any tool other than the built-ins, declared once
    /// per Agent; resolves only the external gateway bindings the graph
    /// selects that are assigned to the caller's tenant; and then hydrates
    /// and prepares the Skald run on the Workflow tracker's blocking pool, so
    /// shutdown waits for that work even when this future is cancelled. The
    /// run's tools are then bound to the total deadline the prepared run
    /// fixed. No provider or tool is called.
    ///
    /// # Errors
    /// Returns the registry and graph-bound errors of
    /// [`PinnedWorkflowGraph::pin`]; `WYRD_WORKFLOW_422_SERVER_NATIVE_UNSUPPORTED`
    /// for a `native` step; `WYRD_WORKFLOW_422_TOOL_UNAVAILABLE` for an
    /// unknown or duplicate tool; `WYRD_WORKFLOW_503_BINDING_UNAVAILABLE`
    /// for an unreadable binding; `WYRD_WORKFLOW_500_INTERNAL` when the
    /// blocking task fails; and the hydration, input, route, and terminal
    /// reserve errors of Skald's preparation.
    async fn prepare(
        state: &AppState,
        caller: Caller,
        request: CreateWorkflowRunRequest,
        timeout: Duration,
        cancel: &CancellationToken,
    ) -> Result<(PreparedWorkflowRun, RunTools), WyrdError> {
        #[cfg(feature = "test-support")]
        state.workflows.pass_preparation_gate_for_test().await;
        let config = state.workflows.config();
        let graph = {
            let mut conn = state.registry_tenant_conn(caller.data_tenant_id).await?;
            let graph = PinnedWorkflowGraph::pin(
                &mut conn,
                &request.workflow,
                GraphBounds {
                    max_steps: config.max_steps_per_run,
                    max_edges: config.max_dependency_edges_per_run,
                    max_bytes: config.max_resolved_graph_bytes,
                },
            )
            .await?;
            conn.commit().await.map_err(registry_db_error)?;
            graph
        };
        let external = Self::external_bindings(&graph)?;
        Self::check_tools(&graph)?;
        let mut bindings = ExternalGatewayBindings::new();
        for name in &external {
            if let Some(configured) = config
                .external_gateway_bindings
                .get(name)
                .filter(|configured| configured.tenant == caller.data_tenant_id)
            {
                bindings.insert(
                    wyrd_client::workflow::resolve_binding(name, &configured.binding).await?,
                )?;
            }
        }
        let profile = if state.deployment_profile.is_production() {
            ExternalEndpointProfile::Production
        } else {
            ExternalEndpointProfile::Local
        };
        let gateway: Arc<dyn WyrdGatewayCaller> =
            Arc::new(ServerWyrdGatewayCaller::new(state.clone(), caller.clone()));
        let dependencies = WorkflowExecutionDependencies::new(ProviderRegistry::default())
            .with_wyrd_gateway(gateway)
            .with_external_gateways(bindings)
            .with_endpoint_profile(profile);
        let options = WorkflowRunOptions {
            limits: WorkflowExecutionLimits {
                max_concurrency: NonZeroUsize::new(config.max_concurrency_per_run).ok_or_else(
                    || WyrdError::WorkflowInternal {
                        message: "workflow.max_concurrency_per_run must be positive".to_owned(),
                        details: serde_json::json!({}),
                    },
                )?,
                deadline: Some(timeout),
                max_input_bytes: Some(config.max_input_bytes),
                max_step_result_bytes: Some(config.max_step_result_bytes),
                max_run_bytes: Some(config.max_run_bytes),
            },
            cancellation: cancel.clone(),
        };
        let tools = RunTools::new(
            state.clone(),
            caller,
            cancel.clone(),
            config.max_step_result_bytes,
        );
        let agent_tools = tools.clone();
        let input = WorkflowInput::Vars(request.input.into_iter().collect());
        let prepared = state
            .workflows
            .spawn_blocking(move || {
                let workflow = SkaldWorkflow::from_card_bodies(
                    graph.workflow().clone(),
                    &|agent| agent_tools.for_agent(agent),
                    &|dependency| graph.body(dependency),
                )?;
                workflow
                    .prepare(&dependencies, input, options)
                    .map_err(WyrdError::from)
            })
            .await
            .map_err(|_| WyrdError::WorkflowInternal {
                message: "Workflow preparation stopped before it finished".to_owned(),
                details: serde_json::json!({ "boundary": "preparation" }),
            })??;
        tools.bind_deadline(&prepared);
        Ok((prepared, tools))
    }

    /// The external gateway bindings the graph's routes select.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_422_SERVER_NATIVE_UNSUPPORTED` naming the first
    /// step that resolves to `native`.
    fn external_bindings(
        graph: &PinnedWorkflowGraph,
    ) -> Result<BTreeSet<CredentialBindingName>, WyrdError> {
        let spec = &graph.workflow().spec;
        let mut external = BTreeSet::new();
        for step in &spec.steps {
            match spec.resolved_route(step) {
                LlmRoute::Native => {
                    return Err(WyrdError::WorkflowServerNativeUnsupported {
                        message: format!("step '{}' uses the native route", step.id),
                        details: serde_json::json!({ "step": step.id }),
                    });
                }
                LlmRoute::WyrdGateway => {}
                LlmRoute::ExtGateway {
                    credential_binding, ..
                } => {
                    external.insert(credential_binding.clone());
                }
            }
        }
        Ok(external)
    }

    /// Check every Agent's declared tools against the built-ins.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_422_TOOL_UNAVAILABLE` naming the first unknown
    /// or repeated tool name.
    fn check_tools(graph: &PinnedWorkflowGraph) -> Result<(), WyrdError> {
        let inline = graph
            .workflow()
            .spec
            .steps
            .iter()
            .filter_map(|step| match &step.action {
                WorkflowAction::Agent(InlineableRef::Inline(agent)) => Some(agent.as_ref()),
                WorkflowAction::Agent(_) => None,
            });
        for agent in inline.chain(graph.agents()) {
            Self::check_agent_tools(agent)?;
        }
        Ok(())
    }

    /// Check one Agent's declared tools.
    ///
    /// # Errors
    /// Returns `WYRD_WORKFLOW_422_TOOL_UNAVAILABLE` naming the first unknown
    /// or repeated tool name.
    fn check_agent_tools(agent: &AgentSpec) -> Result<(), WyrdError> {
        let mut seen = BTreeSet::new();
        for name in &agent.tool_names {
            if !is_builtin(name) || !seen.insert(name.as_str()) {
                return Err(WyrdError::WorkflowToolUnavailable {
                    message: format!("tool '{name}' is unavailable or declared twice"),
                    details: serde_json::json!({ "tool": name }),
                });
            }
        }
        Ok(())
    }
}

/// Audit resource of a create: the Workflow it names.
fn workflow_resource(workflow: &CardRef) -> String {
    format!(
        "workflow:{}/{}@{}",
        workflow.space.as_ref().map_or("-", |space| space.as_str()),
        workflow.name,
        workflow.version,
    )
}
