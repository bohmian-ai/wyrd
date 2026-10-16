//! The server's built-in Agent tools.
//!
//! [`HostedTools`] is the server [`AgentToolset`]: for each Agent step
//! attempt it materializes the declared workspace when a filesystem or code
//! tool needs one, then registers one [`Builtin`] per declared tool bound to
//! that invocation's accepted caller, workspace, deadline, and Python budget.
//! Every call first authorizes and audits `tool:invoke` on the exact built-in
//! through the shared [`audit::authorize`] boundary, then dispatches to the
//! existing authorized owner: the Card read operations, the Bifrost catalog
//! service, [`BoundedQuery`], the [`Workspace`], or the [`Sandbox`]. Those
//! owners still enforce their own permissions, so a tool grant never implies
//! data access. Card and Bifrost descriptions and argument schemas are the
//! MCP catalog's, so both surfaces advertise one contract.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use monty_pool::{MountSpec, MountSpecMode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use skald_tool::{AgentTool, StructuredInvocationError, ToolError, ToolRegistry};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use vala_eval::blocks::{AgentToolset, BlockCall, BlockError, ToolSession};
use wyrd_runtime::Permission;
use wyrd_spec::card::agent::BuiltinTool;
use wyrd_spec::card::blocks::Action;
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::ListCardsRequest;

use super::HostedLimits;
use super::evidence::TenantEvidence;
use super::sandbox::Sandbox;
use super::workspace::{Workspace, Workspaces};
use crate::AppState;
use crate::audit;
use crate::bifrost::service;
use crate::components::auth::Caller;
use crate::components::cards::routes::{get_card_by_ref_for, get_card_for, list_cards_for};
use crate::mcp::bifrost::{DescribeTableArguments, compact_entry};
use crate::mcp::cards::{GetCardArgs, GetCardByRefArgs};
use crate::query::collect::{BoundedQuery, QueryArguments};

/// Audit operation of every tool invocation decision.
const OPERATION: &str = "verification.tool.invoke";

/// Built-in tools for one verification's Agent steps.
pub struct HostedTools {
    /// Server state the authorized owners read through.
    state: AppState,
    /// The verification's accepted caller; every tool call runs as it.
    caller: Caller,
    /// The verification's pinned evidence, for workspaces.
    evidence: Arc<TenantEvidence>,
    /// Process-wide workspace admission.
    workspaces: Arc<Workspaces>,
    /// The process-wide Python sandbox.
    sandbox: Arc<Sandbox>,
    /// Limits in force.
    limits: HostedLimits,
}

impl HostedTools {
    /// Tools running as `caller` over `evidence`.
    #[must_use]
    pub const fn new(
        state: AppState,
        caller: Caller,
        evidence: Arc<TenantEvidence>,
        workspaces: Arc<Workspaces>,
        sandbox: Arc<Sandbox>,
        limits: HostedLimits,
    ) -> Self {
        Self {
            state,
            caller,
            evidence,
            workspaces,
            sandbox,
            limits,
        }
    }
}

#[async_trait]
impl AgentToolset for HostedTools {
    /// Materialize the workspace when a declared tool needs one, then bind
    /// each declared tool to this attempt.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] when a filesystem or code tool is
    /// declared without a workspace, plus [`Workspaces::open`] failures.
    async fn open(
        &self,
        call: &BlockCall<'_, Action>,
        tools: &[BuiltinTool],
    ) -> Result<ToolSession, BlockError> {
        let workspace = if tools
            .iter()
            .any(|tool| tool.is_filesystem() || *tool == BuiltinTool::CodeRun)
        {
            if call.workspace.is_empty() {
                return Err(BlockError::Terminal {
                    reason: "filesystem and code tools need a declared workspace".to_owned(),
                });
            }
            Some(Arc::new(
                self.workspaces
                    .open(&call.workspace, &self.evidence)
                    .await?,
            ))
        } else {
            None
        };
        let invocation = Arc::new(Invocation {
            state: self.state.clone(),
            caller: self.caller.clone(),
            workspace,
            sandbox: Arc::clone(&self.sandbox),
            deadline: call.deadline,
            code_budget: Mutex::new(self.limits.agent_code_budget),
            limits: self.limits,
        });
        let registry = ToolRegistry::new();
        for &tool in tools {
            let (description, input_schema) = descriptor(tool);
            registry
                .register(Arc::new(Builtin {
                    tool,
                    description,
                    input_schema,
                    invocation: Arc::clone(&invocation),
                }))
                .map_err(|error| BlockError::Terminal {
                    reason: error.to_string(),
                })?;
        }
        Ok(ToolSession::new(
            registry,
            self.limits.tool_result_max_bytes,
            Box::new(invocation),
        ))
    }
}

/// One Agent step attempt's tool state; dropped with its session.
struct Invocation {
    /// Server state.
    state: AppState,
    /// The accepted caller.
    caller: Caller,
    /// The attempt's workspace, when a declared tool needs one.
    workspace: Option<Arc<Workspace>>,
    /// The shared sandbox.
    sandbox: Arc<Sandbox>,
    /// When the engine cancels the attempt.
    deadline: Instant,
    /// Python time left for `code.run` in this attempt.
    code_budget: Mutex<Duration>,
    /// Limits in force.
    limits: HostedLimits,
}

impl Invocation {
    /// Run `tool` with `args` as the accepted caller.
    ///
    /// # Errors
    /// Returns the tool-invoke denial before anything runs, then argument,
    /// domain authorization, and execution failures.
    async fn call(&self, tool: BuiltinTool, args: Value) -> Result<Value, ToolError> {
        audit::authorize(
            &self.state,
            &self.caller,
            &Permission::tool_invoke(tool),
            OPERATION,
            tool.as_str(),
        )
        .map_err(|error| structured(&error))?;
        match tool {
            BuiltinTool::CardsList => {
                let request: ListCardsRequest = parse(args)?;
                to_value(list_cards_for(&self.state, &self.caller, request).await)
            }
            BuiltinTool::CardsGet => {
                let args: GetCardArgs = parse(args)?;
                to_value(get_card_for(&self.state, &self.caller, &args.kind, &args.card_uid).await)
            }
            BuiltinTool::CardsGetByRef => {
                let args: GetCardByRefArgs = parse(args)?;
                to_value(get_card_by_ref_for(&self.state, &self.caller, &args.card_ref).await)
            }
            BuiltinTool::BifrostListTables => {
                let Empty {} = parse(args)?;
                let tables = service::list_tables(&self.state, self.caller.clone())
                    .await
                    .map_err(|error| structured(&error))?;
                Ok(json!({"tables": tables.iter().map(compact_entry).collect::<Vec<_>>()}))
            }
            BuiltinTool::BifrostDescribeTable => {
                let args: DescribeTableArguments = parse(args)?;
                to_value(
                    service::describe_table(
                        &self.state,
                        self.caller.clone(),
                        args.namespace,
                        args.name,
                    )
                    .await,
                )
            }
            BuiltinTool::BifrostQuery => {
                let args: QueryArguments = parse(args)?;
                args.validate()
                    .map_err(|invalid| ToolError::InvalidInput(invalid.reason.to_owned()))?;
                // ponytail: never cancelled; the engine drops the call at its
                // deadline and the query keeps its own `deadline_ms`.
                BoundedQuery::new(self.state.clone())
                    .run(self.caller.clone(), &args, &CancellationToken::new())
                    .await
                    .map_err(|error| structured(&error))
            }
            BuiltinTool::FsList | BuiltinTool::FsRead | BuiltinTool::FsSearch => {
                let workspace = Arc::clone(self.workspace()?);
                let args: FsArgs = parse(args)?;
                tokio::task::spawn_blocking(move || match tool {
                    BuiltinTool::FsList => workspace.list(&args.path),
                    BuiltinTool::FsRead => workspace.read(&args.path, args.start),
                    _ => workspace.search(&args.query),
                })
                .await
                .map_err(|error| invocation(error.to_string()))?
                .map_err(|error| invocation(error.to_string()))
            }
            BuiltinTool::CodeRun => {
                let CodeArgs { source } = parse(args)?;
                self.run(source).await
            }
        }
    }

    /// Run `source` with `/inputs` read-only and `/work` writable, charging
    /// its time to the attempt's Python budget.
    ///
    /// # Errors
    /// Returns [`ToolError::Invocation`] when the budget is spent, a mount
    /// cannot open, or the sandbox refuses or fails the code.
    async fn run(&self, source: String) -> Result<Value, ToolError> {
        let workspace = self.workspace()?;
        let budget = *self
            .code_budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if budget.is_zero() {
            return Err(invocation(
                "this call's Python time budget is spent".to_owned(),
            ));
        }
        let mount = |path: &str, host, mode| {
            MountSpec::new(path, host, mode).map_err(|error| invocation(error.to_string()))
        };
        let mut work = mount("/work", workspace.work_dir(), MountSpecMode::ReadWrite)?;
        work.write_bytes_limit = Some(self.limits.work_write_max_bytes);
        let mounts = vec![
            mount("/inputs", workspace.inputs_dir(), MountSpecMode::ReadOnly)?,
            work,
        ];
        let started = Instant::now();
        let ran = self
            .sandbox
            .exec(
                source,
                Vec::new(),
                mounts,
                self.deadline.min(started + budget),
            )
            .await;
        let mut left = self
            .code_budget
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        *left = left.saturating_sub(started.elapsed());
        let (value, output) = ran.map_err(|error| invocation(error.to_string()))?;
        Ok(json!({"value": value.py_repr(), "output": output}))
    }

    /// The attempt's workspace.
    ///
    /// # Errors
    /// Returns [`ToolError::Invocation`] when the attempt has none, which
    /// [`HostedTools::open`] prevents for every tool that needs one.
    fn workspace(&self) -> Result<&Arc<Workspace>, ToolError> {
        self.workspace
            .as_ref()
            .ok_or_else(|| invocation("this call has no workspace".to_owned()))
    }
}

/// One declared built-in bound to an invocation.
struct Builtin {
    /// The built-in.
    tool: BuiltinTool,
    /// What the model is told it does.
    description: String,
    /// Its closed argument schema.
    input_schema: Value,
    /// The invocation it runs in.
    invocation: Arc<Invocation>,
}

#[async_trait]
impl AgentTool for Builtin {
    /// The wire name; the Agent executor renames it for providers.
    fn name(&self) -> &str {
        self.tool.as_str()
    }

    /// The tool description.
    fn description(&self) -> &str {
        &self.description
    }

    /// The closed argument schema.
    fn input_schema(&self) -> Value {
        self.input_schema.clone()
    }

    /// Any JSON object.
    fn output_schema(&self) -> Value {
        json!({"type": "object"})
    }

    /// Authorize, then run the tool.
    ///
    /// # Errors
    /// Returns [`Invocation::call`] failures.
    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        self.invocation.call(self.tool, args).await
    }
}

/// Arguments of `bifrost.list_tables`: none.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

/// Arguments of the filesystem tools; each reads only its own fields.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FsArgs {
    /// Workspace path for `fs.list` and `fs.read`.
    #[serde(default)]
    path: String,
    /// First unit `fs.read` returns.
    #[serde(default)]
    start: usize,
    /// Case-insensitive text `fs.search` looks for.
    #[serde(default)]
    query: String,
}

/// Arguments of `code.run`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CodeArgs {
    /// Python whose last expression is the result.
    source: String,
}

/// The description and argument schema of `tool`; Card and Bifrost tools
/// reuse the MCP catalog's.
fn descriptor(tool: BuiltinTool) -> (String, Value) {
    let path =
        json!({"type": "string", "description": "Absolute workspace path under /inputs or /work."});
    let (description, schema) = match tool {
        BuiltinTool::FsList => (
            "List the files under a workspace directory.",
            json!({"type": "object", "properties": {"path": path}, "required": ["path"], "additionalProperties": false}),
        ),
        BuiltinTool::FsRead => (
            "Read a workspace file as located text units (page, slide, line, or JSON pointer), \
             starting at unit `start`; follow `next` to continue.",
            json!({"type": "object", "properties": {"path": path, "start": {"type": "integer", "minimum": 0}}, "required": ["path"], "additionalProperties": false}),
        ),
        BuiltinTool::FsSearch => (
            "Search every workspace file for text, case-insensitively, returning each match's path \
             and locator.",
            json!({"type": "object", "properties": {"query": {"type": "string", "minLength": 1}}, "required": ["query"], "additionalProperties": false}),
        ),
        BuiltinTool::CodeRun => (
            "Run sandboxed Python with /inputs read-only and /work writable; returns printed output \
             and the repr of the last expression. No network, host files, or packages.",
            json!({"type": "object", "properties": {"source": {"type": "string"}}, "required": ["source"], "additionalProperties": false}),
        ),
        _ => {
            return crate::mcp::cards::descriptors()
                .into_iter()
                .chain(crate::mcp::bifrost::descriptors())
                .find(|descriptor| descriptor.name == tool.as_str())
                .map(|descriptor| {
                    (
                        descriptor.description.unwrap_or_default().into_owned(),
                        Value::Object((*descriptor.input_schema).clone()),
                    )
                })
                .expect("every Card and Bifrost built-in has an MCP descriptor");
        }
    };
    (description.to_owned(), schema)
}

/// Deserialize `args` into a tool's closed arguments.
///
/// # Errors
/// Returns [`ToolError::InvalidInput`] for unknown, missing, or mistyped
/// fields, including any attempt to pass authentication context.
fn parse<T: DeserializeOwned>(args: Value) -> Result<T, ToolError> {
    serde_json::from_value(args).map_err(|error| ToolError::InvalidInput(error.to_string()))
}

/// Serialize an owner's answer, keeping its error's stable problem.
///
/// # Errors
/// Returns the owner's error as [`structured`], or
/// [`ToolError::OutputSerialization`].
fn to_value<T: serde::Serialize>(result: Result<T, WyrdError>) -> Result<Value, ToolError> {
    serde_json::to_value(result.map_err(|error| structured(&error))?)
        .map_err(|error| ToolError::OutputSerialization(error.to_string()))
}

/// A Wyrd error as a structured tool failure with its stable code.
fn structured(error: &WyrdError) -> ToolError {
    let problem = error.problem();
    ToolError::StructuredInvocation(Box::new(StructuredInvocationError {
        code: problem.code,
        status: problem.status,
        title: problem.title,
        detail: problem.detail,
        remediation: problem.remediation,
        safe_details: Some(problem.details),
    }))
}

/// A plain tool failure with `detail`.
fn invocation(detail: String) -> ToolError {
    ToolError::Invocation {
        detail,
        cause: None,
    }
}

#[cfg(test)]
mod tests;
