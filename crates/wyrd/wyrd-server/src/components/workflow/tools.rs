//! The built-in read tools an accepted server Workflow's Agents may declare.
//!
//! [`RunTools`] binds `bifrost.query` and `cards.get` to one accepted run's
//! captured caller. Each call authorizes and audits through the owning
//! service exactly as an HTTP or MCP read does: queries through
//! [`BoundedQuery`] and Card reads through
//! [`get_card_by_ref_for`]. A query runs on the run's tracked tool-owner
//! work, not inside the Agent step, so aborting the step only signals the
//! query; the owner still settles it under its own deadline, and the run
//! commits its terminal snapshot only after every owner finished. Failures
//! reach the model as the owner's stable code and title alone.

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use skald_tool::{AgentTool, StructuredInvocationError, ToolError, ToolResolver};
use skald_workflow::PreparedWorkflowRun;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use wyrd_semver::VersionBlock;
use wyrd_spec::envelope::{Card, CardKind};
use wyrd_spec::error::WyrdError;
use wyrd_spec::ids::{CardName, CardUid, SpaceName};
use wyrd_spec::reference::CardRef;
use wyrd_spec::request_id::RequestId;

use crate::components::auth::Caller;
use crate::components::cards::routes::get_card_by_ref_for;
use crate::query::collect::{BoundedQuery, QUERY, QueryArguments};
use crate::state::AppState;

/// Wire name of the exact Card read tool.
pub(crate) const CARDS_GET: &str = "cards.get";

/// Whether `name` is a built-in tool a server Workflow Agent may declare.
pub(crate) fn is_builtin(name: &str) -> bool {
    name == QUERY || name == CARDS_GET
}

/// One accepted run's built-in tools.
///
/// Cloning is cheap; every clone shares the run's tool-owner tracker,
/// cancellation, and deadline.
#[derive(Clone)]
pub(crate) struct RunTools {
    /// Server state whose query and Cards services serve every call.
    state: AppState,
    /// Authority captured when the run was accepted.
    caller: Caller,
    /// The run's cancellation; each query owner's token is its child.
    cancel: CancellationToken,
    /// Tracks the run's query owners until they settle.
    owners: TaskTracker,
    /// Largest serialized result one call may return.
    max_result_bytes: usize,
    /// The prepared run's absolute total deadline, bound once by
    /// [`Self::bind_deadline`] after the Agents holding these clones were
    /// hydrated and before any tool can be called.
    deadline: Arc<OnceLock<Instant>>,
}

impl RunTools {
    /// Bind the built-in tools to one run.
    ///
    /// The run's deadline is not known yet: Skald fixes it only once the
    /// Agents holding these tools are hydrated and the run is prepared, and
    /// [`Self::bind_deadline`] then hands it to every clone.
    pub(crate) fn new(
        state: AppState,
        caller: Caller,
        cancel: CancellationToken,
        max_result_bytes: usize,
    ) -> Self {
        Self {
            state,
            caller,
            cancel,
            owners: TaskTracker::new(),
            max_result_bytes,
            deadline: Arc::default(),
        }
    }

    /// Bind every clone to the absolute total deadline `prepared` fixed.
    ///
    /// Called once, after preparation and before the run is accepted, so a
    /// query is clipped to exactly the instant at which the run times out. A
    /// run without a deadline binds nothing, and a second binding is ignored.
    pub(crate) fn bind_deadline(&self, prepared: &PreparedWorkflowRun) {
        if let Some(deadline) = prepared.deadline() {
            self.deadline.get_or_init(|| deadline);
        }
    }

    /// The tools of the Agent `agent` names, whose space fills an omitted
    /// `cards.get` space.
    pub(crate) fn for_agent(&self, agent: &CardRef) -> Box<dyn ToolResolver> {
        Box::new(AgentRunTools {
            tools: self.clone(),
            space: agent.space.clone(),
        })
    }

    /// Wait until every query owner of the run has settled.
    ///
    /// Called once the executor returned, when no further tool call can
    /// start.
    pub(crate) async fn drain(&self) {
        self.owners.close();
        self.owners.wait().await;
    }

    /// The captured caller under a fresh request id, so concurrent calls of
    /// one run are distinct queries to the query controls.
    fn call_caller(&self) -> Caller {
        Caller {
            request_id: RequestId::now_v7(),
            ..self.caller.clone()
        }
    }
}

/// The built-in tools as one Agent sees them.
struct AgentRunTools {
    /// The run's tools.
    tools: RunTools,
    /// The executing Agent's space.
    space: Option<SpaceName>,
}

impl ToolResolver for AgentRunTools {
    /// Resolve a built-in tool by name.
    ///
    /// # Errors
    /// Returns [`ToolError::NotRegistered`] for any other name.
    fn resolve(&self, name: &str) -> Result<Arc<dyn AgentTool>, ToolError> {
        match name {
            QUERY => Ok(Arc::new(QueryTool {
                tools: self.tools.clone(),
            })),
            CARDS_GET => Ok(Arc::new(CardsTool {
                tools: self.tools.clone(),
                space: self.space.clone(),
            })),
            _ => Err(ToolError::NotRegistered {
                name: name.to_owned(),
                available: vec![CARDS_GET.to_owned(), QUERY.to_owned()],
            }),
        }
    }
}

/// `bifrost.query`: one bounded complete-result read-only query.
struct QueryTool {
    /// The run's tools.
    tools: RunTools,
}

#[async_trait]
impl AgentTool for QueryTool {
    /// The stable wire name `bifrost.query`, the same name MCP serves and
    /// Agent Cards declare.
    fn name(&self) -> &str {
        QUERY
    }

    /// The provider-visible description: one read-only SELECT over the
    /// caller's tenant, returning a complete, never truncated result.
    fn description(&self) -> &str {
        "Run one read-only SELECT over this tenant's Bifrost tables and return the complete result as {columns, rows, terminal}."
    }

    /// The closed argument schema MCP also advertises; [`QueryArguments`]
    /// enforces the same bounds when the call arrives.
    fn input_schema(&self) -> Value {
        crate::query::collect::input_schema()
    }

    /// The closed `{columns, rows, terminal}` result schema of
    /// [`BoundedQuery::run`], the only success value this tool returns.
    fn output_schema(&self) -> Value {
        crate::query::collect::output_schema()
    }

    /// Run the query on a tracked owner and wait for its complete result.
    ///
    /// The result ceiling is the smaller of the requested `max_bytes` and the
    /// run's step-result bound, and the deadline the smaller of the requested
    /// one and the time remaining until the prepared run's deadline. Dropping
    /// this future cancels the owner's token; the owner keeps the response
    /// and settles it.
    ///
    /// # Errors
    /// Returns a redacted structured failure for malformed or out-of-range
    /// arguments and for every authorization, audit, query, stream, and
    /// result-ceiling failure of [`BoundedQuery::run`].
    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let mut arguments: QueryArguments =
            serde_json::from_value(args).map_err(|_| invalid_input(QUERY))?;
        arguments.validate().map_err(|_| invalid_input(QUERY))?;
        arguments.max_bytes = arguments.max_bytes.min(self.tools.max_result_bytes);
        if let Some(deadline) = self.tools.deadline.get() {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .as_millis();
            let remaining = u32::try_from(remaining).unwrap_or(u32::MAX).max(1);
            arguments.deadline_ms = Some(
                arguments
                    .deadline_ms
                    .map_or(remaining, |requested| requested.min(remaining)),
            );
        }
        let cancel = self.tools.cancel.child_token();
        let waiter = cancel.clone().drop_guard();
        let query = BoundedQuery::new(self.tools.state.clone());
        let caller = self.tools.call_caller();
        let owner = self
            .tools
            .owners
            .spawn(async move { query.run(caller, &arguments, &cancel).await });
        let outcome = owner.await;
        waiter.disarm();
        match outcome {
            Ok(result) => result.map_err(|error| failure(&error)),
            Err(_) => Err(failure(&WyrdError::ToolInvocationFailed {
                message: format!("{QUERY} stopped before settling"),
                details: serde_json::json!({ "tool": QUERY }),
            })),
        }
    }
}

/// Closed arguments of `cards.get`: one exact Card reference.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CardsGetArguments {
    /// Card kind.
    kind: CardKind,
    /// Card name.
    name: CardName,
    /// Exact Card version.
    version: VersionBlock,
    /// Card space; the executing Agent's space when omitted.
    #[serde(default)]
    space: Option<SpaceName>,
    /// Card UID the reference must name.
    #[serde(default)]
    uid: Option<CardUid>,
}

/// `cards.get`: one authorized exact Card read.
struct CardsTool {
    /// The run's tools.
    tools: RunTools,
    /// The executing Agent's space.
    space: Option<SpaceName>,
}

#[async_trait]
impl AgentTool for CardsTool {
    /// The stable wire name `cards.get` Agent Cards declare.
    fn name(&self) -> &str {
        CARDS_GET
    }

    /// The provider-visible description: one exact Card read whose space
    /// defaults to the executing Agent's space.
    fn description(&self) -> &str {
        "Read one registered Card by its exact kind, name, and version; space defaults to this Agent's space."
    }

    /// The closed schema of [`CardsGetArguments`]: one exact Card reference
    /// whose optional space and UID narrow it, with no other field accepted.
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "description": "Card kind." },
                "name": { "type": "string", "description": "Card name." },
                "version": { "type": "string", "description": "Exact Card version." },
                "space": { "type": "string", "description": "Card space; defaults to this Agent's space." },
                "uid": { "type": "string", "description": "Card UID the reference must name." }
            },
            "required": ["kind", "name", "version"],
            "additionalProperties": false
        })
    }

    /// The schema derived from the canonical [`Card`] envelope, which is
    /// exactly what a successful read serializes.
    ///
    /// # Panics
    /// Never in practice: a derived schema always serializes to JSON.
    fn output_schema(&self) -> Value {
        serde_json::to_value(schemars::schema_for!(Card)).expect("a derived schema is JSON")
    }

    /// Read the Card through the authorized, audited Cards read.
    ///
    /// # Errors
    /// Returns a redacted structured failure for malformed arguments and for
    /// every authorization, audit, and registry failure of
    /// [`get_card_by_ref_for`].
    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let arguments: CardsGetArguments =
            serde_json::from_value(args).map_err(|_| invalid_input(CARDS_GET))?;
        let card_ref = CardRef {
            kind: arguments.kind,
            name: arguments.name,
            version: arguments.version,
            space: arguments.space.or_else(|| self.space.clone()),
            uid: arguments.uid,
        };
        let response = get_card_by_ref_for(&self.tools.state, &self.tools.call_caller(), &card_ref)
            .await
            .map_err(|error| failure(&error))?;
        serde_json::to_value(response.card).map_err(|_| {
            failure(&WyrdError::ToolOutputSerialization {
                message: format!("{CARDS_GET} could not serialize the Card"),
                details: serde_json::json!({ "tool": CARDS_GET }),
            })
        })
    }
}

/// The redacted failure for arguments that do not match `tool`'s schema.
fn invalid_input(tool: &str) -> ToolError {
    failure(&WyrdError::ToolInvalidInput {
        message: format!("{tool} arguments do not match its input schema"),
        details: serde_json::json!({ "tool": tool }),
    })
}

/// Project `error` onto a non-retryable tool failure carrying only its
/// stable code, status, title, and remediation.
///
/// The model sees the failure, so nothing caller- or data-derived from the
/// error's message or details is kept.
fn failure(error: &WyrdError) -> ToolError {
    ToolError::StructuredInvocation(Box::new(StructuredInvocationError {
        code: error.code().to_owned(),
        status: error.status(),
        title: error.title().to_owned(),
        detail: error.title().to_owned(),
        remediation: error.remediation().to_owned(),
        safe_details: None,
    }))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use serde_json::json;
    use skald_runtime::ProviderRegistry;
    use skald_workflow::{
        Workflow, WorkflowExecutionDependencies, WorkflowExecutionLimits, WorkflowInput,
        WorkflowRunOptions,
    };
    use tokio_util::sync::CancellationToken;
    use wyrd_spec::DataTenantId;
    use wyrd_spec::card::workflow::WorkflowCard;

    use super::RunTools;
    use crate::components::cards::service::reconciliation_caller;
    use crate::state::AppState;

    /// A server state over the shared test fixture, enough to construct run
    /// tools that are never called.
    ///
    /// # Panics
    /// Panics when the shared Postgres, storage, or catalog fixtures cannot
    /// start.
    async fn state() -> AppState {
        crate::test_support::test_app_state(
            crate::test_support::test_server_postgres().await,
            crate::test_support::test_storage().await,
            crate::test_support::test_catalog().await,
        )
    }

    /// One inline Agent step that declares `bifrost.query`.
    fn card() -> WorkflowCard {
        serde_json::from_value(json!({
            "apiVersion": "wyrd/v1",
            "kind": "Workflow",
            "metadata": { "space": "engineering", "name": "deadline", "version": "1.0.0" },
            "spec": {
                "steps": [{
                    "id": "count",
                    "action": {
                        "type": "agent",
                        "target": {
                            "prompt": {
                                "model": "gpt-5-5",
                                "request": {
                                    "provider": "open_ai_chat_completion",
                                    "body": {
                                        "model": "gpt-5-5",
                                        "messages": [{ "role": "user", "content": "Count them." }]
                                    }
                                },
                                "response_type": "text"
                            },
                            "tool_names": ["bifrost.query"]
                        }
                    }
                }],
                "outputs": { "answer": "steps.count.output.text" }
            }
        }))
        .expect("fixture Workflow Card parses")
    }

    /// Tools created before hydration and planning carry exactly the deadline
    /// the prepared run fixes afterwards, not one sampled when they were
    /// created.
    ///
    /// # Panics
    /// Panics if the fixture does not prepare or the tools hold any other
    /// deadline.
    #[tokio::test(start_paused = true)]
    async fn tools_use_the_prepared_run_deadline() {
        let tools = RunTools::new(
            state().await,
            reconciliation_caller(DataTenantId::new_v7()),
            CancellationToken::new(),
            1024,
        );
        let agent_tools = tools.clone();
        let workflow =
            Workflow::from_card_bodies(card(), &|agent| agent_tools.for_agent(agent), &|_| None)
                .expect("fixture Workflow hydrates");
        tokio::time::advance(Duration::from_secs(1)).await;
        let options = WorkflowRunOptions {
            limits: WorkflowExecutionLimits {
                max_concurrency: NonZeroUsize::MIN,
                deadline: Some(Duration::from_secs(30)),
                max_input_bytes: None,
                max_step_result_bytes: None,
                max_run_bytes: None,
            },
            cancellation: CancellationToken::new(),
        };
        let prepared = workflow
            .prepare(
                &WorkflowExecutionDependencies::new(ProviderRegistry::default()),
                WorkflowInput::Vars(serde_json::Map::new()),
                options,
            )
            .expect("fixture Workflow prepares");

        tools.bind_deadline(&prepared);

        assert!(prepared.deadline().is_some(), "the run has a deadline");
        assert_eq!(
            agent_tools.deadline.get().copied(),
            prepared.deadline(),
            "every clone holds the prepared run's deadline"
        );
    }
}
