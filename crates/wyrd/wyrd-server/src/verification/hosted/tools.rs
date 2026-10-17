//! The server's built-in Agent tools.
//!
//! [`HostedTools`] is the server [`AgentToolset`]: for each Agent step or check
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

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use async_trait::async_trait;
use monty_types::MontyObject;
use monty_types::format::StringRepr;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use skald_tool::{AgentTool, StructuredInvocationError, ToolError, ToolRegistry};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use vala_verify::blocks::{AgentToolset, BlockError, CallInput, ToolSession};
use wyrd_runtime::Permission;
use wyrd_spec::card::agent::BuiltinTool;
use wyrd_spec::error::WyrdError;
use wyrd_spec::registry::ListCardsRequest;
use wyrd_spec::verification::TaskId;

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
    /// Python time left per Agent action, kept across its attempts so a
    /// retry never restores spent time.
    code_budgets: Mutex<BTreeMap<TaskId, Arc<Mutex<Duration>>>>,
    /// Limits in force.
    limits: HostedLimits,
}

impl HostedTools {
    /// Tools running as `caller` over `evidence` for one execution; each
    /// action's Python budget starts full here and only ever shrinks.
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
            code_budgets: Mutex::new(BTreeMap::new()),
            limits,
        }
    }

    /// The Python budget of action `id`, created full on its first attempt.
    fn code_budget(&self, id: &TaskId) -> Arc<Mutex<Duration>> {
        Arc::clone(
            self.code_budgets
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(id.clone())
                .or_insert_with(|| Arc::new(Mutex::new(self.limits.agent_code_budget))),
        )
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
        id: &TaskId,
        workspace: &BTreeMap<String, CallInput>,
        deadline: Instant,
        tools: &[BuiltinTool],
    ) -> Result<ToolSession, BlockError> {
        let workspace = if tools
            .iter()
            .any(|tool| tool.is_filesystem() || *tool == BuiltinTool::CodeRun)
        {
            if workspace.is_empty() {
                return Err(BlockError::Terminal {
                    reason: "filesystem and code tools need a declared workspace".to_owned(),
                });
            }
            Some(Arc::new(
                self.workspaces.open(workspace, &self.evidence).await?,
            ))
        } else {
            None
        };
        let invocation = Arc::new(Invocation {
            state: self.state.clone(),
            caller: self.caller.clone(),
            workspace,
            sandbox: Arc::clone(&self.sandbox),
            deadline,
            code_budget: self.code_budget(id),
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
    /// Python time left for `code.run` in this action, shared with its
    /// other attempts.
    code_budget: Arc<Mutex<Duration>>,
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
                    .run(
                        super::statement_caller(&self.caller),
                        &args,
                        &CancellationToken::new(),
                    )
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
    /// its time to the action's Python budget.
    ///
    /// Time is reserved before waiting for `/work` or a worker, so
    /// concurrent calls never share the same remainder; the [`Reservation`]
    /// returns the unused part when the call ends or is cancelled. `/work`
    /// writers run one at a time, each limited to the space
    /// [`Workspace::mounts`] measures before it starts.
    ///
    /// # Errors
    /// Returns [`ToolError::Invocation`] when the budget is spent, a mount
    /// cannot open, or the sandbox refuses or fails the code. A sandbox
    /// capacity refusal keeps its [`BlockError::Busy`] as the typed cause, so
    /// the Agent session stops the invocation busy rather than handing the
    /// refusal to the model.
    async fn run(&self, source: String) -> Result<Value, ToolError> {
        let workspace = Arc::clone(self.workspace()?);
        let reservation = Reservation::take(&self.code_budget, self.limits.code_timeout)
            .ok_or_else(|| invocation("this call's Python time budget is spent".to_owned()))?;
        let deadline = self
            .deadline
            .min(reservation.started + reservation.reserved);
        let _writer = workspace.writer().await;
        let mounts = {
            let workspace = Arc::clone(&workspace);
            tokio::task::spawn_blocking(move || workspace.mounts())
                .await
                .map_err(|error| invocation(error.to_string()))?
                .map_err(|error| invocation(error.to_string()))?
        };
        let (value, output) = self
            .sandbox
            .exec(source, Vec::new(), mounts, deadline)
            .await
            .map_err(|error| match error {
                BlockError::Busy { .. } => ToolError::Invocation {
                    detail: error.to_string(),
                    cause: Some(Box::new(error)),
                },
                error => invocation(error.to_string()),
            })?;
        let (repr, whole) = repr(&value, self.limits.tool_result_max_bytes);
        Ok(json!({"value": repr, "value_truncated": !whole, "output": output}))
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

/// Python time taken from an action's budget for one `code.run`.
struct Reservation {
    /// The action's remaining budget.
    budget: Arc<Mutex<Duration>>,
    /// Time taken out of it.
    reserved: Duration,
    /// When the reservation was taken.
    started: Instant,
}

impl Reservation {
    /// Take up to `most` from `budget`, or `None` when it is spent.
    fn take(budget: &Arc<Mutex<Duration>>, most: Duration) -> Option<Self> {
        let mut left = budget.lock().unwrap_or_else(PoisonError::into_inner);
        let reserved = left.min(most);
        if reserved.is_zero() {
            return None;
        }
        *left -= reserved;
        Some(Self {
            budget: Arc::clone(budget),
            reserved,
            started: Instant::now(),
        })
    }
}

impl Drop for Reservation {
    /// Return the unused part; elapsed time stays spent whether the call
    /// finished, failed, or was cancelled.
    fn drop(&mut self) {
        let unused = self.reserved.saturating_sub(self.started.elapsed());
        *self.budget.lock().unwrap_or_else(PoisonError::into_inner) += unused;
    }
}

/// Text formatted up to `max` bytes; a write that would pass it keeps the
/// part that fits and fails, which stops the formatter's traversal there.
struct Capped {
    /// What fit.
    text: String,
    /// Byte limit.
    max: usize,
}

impl Capped {
    /// Empty text limited to `max` bytes.
    const fn new(max: usize) -> Self {
        Self {
            text: String::new(),
            max,
        }
    }
}

impl fmt::Write for Capped {
    /// Append `piece`, or the part of it that fits and fail.
    ///
    /// # Errors
    /// Returns [`fmt::Error`] once the limit is reached.
    fn write_str(&mut self, piece: &str) -> fmt::Result {
        let room = self.max - self.text.len();
        if piece.len() <= room {
            self.text.push_str(piece);
            return Ok(());
        }
        self.text
            .push_str(&piece[..piece.floor_char_boundary(room)]);
        Err(fmt::Error)
    }
}

/// Python's `repr` of `value`, cut at `max` bytes, and whether it is whole.
///
/// Formats through Monty's borrowed `Display` (and [`StringRepr`] for
/// strings, whose `Display` is `str`) into a [`Capped`] writer, so a large
/// value stops formatting at the limit instead of building its full repr.
fn repr(value: &MontyObject, max: usize) -> (String, bool) {
    let mut text = Capped::new(max);
    let whole = match value.as_ref().as_str() {
        Some(string) => write!(text, "{}", StringRepr(string)),
        None => write!(text, "{value}"),
    }
    .is_ok();
    (text.text, whole)
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
///
/// # Panics
/// Panics when the MCP catalog lacks a Card or Bifrost built-in, an
/// invariant the catalog's own tests hold.
fn descriptor(tool: BuiltinTool) -> (String, Value) {
    let path =
        json!({"type": "string", "description": "Absolute workspace path under /inputs or /work."});
    let (description, schema) = match tool {
        BuiltinTool::FsList => (
            "List the files under a workspace directory with their sizes; each /inputs file \
             names the original input slots it came from in `sources`.",
            json!({"type": "object", "properties": {"path": path}, "required": ["path"], "additionalProperties": false}),
        ),
        BuiltinTool::FsRead => (
            "Read a workspace file as located text units (page, slide, line, or JSON pointer), \
             starting at unit `start`; follow `next` to continue, since long units arrive in \
             pieces sharing one locator. Cite an /inputs file by one of its `sources` slots \
             with the unit's locator.",
            json!({"type": "object", "properties": {"path": path, "start": {"type": "integer", "minimum": 0}}, "required": ["path"], "additionalProperties": false}),
        ),
        BuiltinTool::FsSearch => (
            "Search every workspace file for text, case-insensitively, returning each match's path, \
             locator, and the original input slots (`sources`) of /inputs files.",
            json!({"type": "object", "properties": {"query": {"type": "string", "minLength": 1}}, "required": ["query"], "additionalProperties": false}),
        ),
        BuiltinTool::CodeRun => (
            "Run sandboxed Python with /inputs read-only and /work writable; returns printed output \
             and the repr of the last expression, both cut at the result limit. No network, host \
             files, or packages.",
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
mod pg_tests;

/// Python time is reserved before a call and settled when it ends.
#[cfg(test)]
mod tests {
    use super::*;

    /// Concurrent reservations split the budget instead of each seeing all
    /// of it; dropping one returns only its unused time.
    ///
    /// # Panics
    /// Panics when reservations overbook the budget or refund spent time.
    #[tokio::test(start_paused = true)]
    async fn reservations_never_overbook_the_budget() {
        let budget = Arc::new(Mutex::new(Duration::from_secs(3)));
        let most = Duration::from_secs(2);
        let first = Reservation::take(&budget, most).expect("first");
        let second = Reservation::take(&budget, most).expect("second");
        assert_eq!(
            (first.reserved, second.reserved),
            (most, Duration::from_secs(1))
        );
        assert!(Reservation::take(&budget, most).is_none());
        tokio::time::advance(Duration::from_millis(1500)).await;
        drop(first);
        drop(second);
        assert_eq!(*budget.lock().expect("budget"), Duration::from_millis(500));
    }

    /// Reprs keep Python's quoting and stop at the limit on a character
    /// boundary, for strings and containers alike.
    ///
    /// # Panics
    /// Panics when a repr changes or overflows the limit.
    #[test]
    fn reprs_stop_at_the_limit() {
        assert_eq!(
            repr(&MontyObject::string("it's"), 64),
            ("\"it's\"".to_owned(), true)
        );
        let big = MontyObject::list((0..100_000).map(|_| MontyObject::string("é\n")));
        let (text, whole) = repr(&big, 64);
        assert!(!whole);
        assert!(text.len() <= 64 && text.starts_with("['é\\n', "), "{text}");
        let (text, whole) = repr(&MontyObject::string("é".repeat(100)), 7);
        assert_eq!((text.as_str(), whole), ("'ééé", false));
    }
}
