//! The tool seam an Agent step action or check uses.
//!
//! The host binds executable built-ins to one attempt's authority, deadline,
//! and workspace and hands them over as a [`ToolSession`]: a Skald
//! [`ToolRegistry`] holding only the tools the Agent declared, the optional
//! [`CodeTool`] that runs `code.run` with a [`ToolBridge`], and a guard that
//! releases the attempt's resources when dropped. The Agent executor resolves
//! each declared name through the registry and attaches recording tools to
//! the ordinary Skald Agent loop.
//!
//! Direct calls from the model and nested `call_tool` requests from
//! `code.run` share one path: each request first spends one unit of the
//! logical invocation's [`ToolCalls`] allowance, then its arguments are
//! bounded, every operation except `code.run` holds one of the session's
//! concurrency permits, and the admitted call is recorded in start order
//! with the recipient its result goes to. A request past the allowance, or a
//! host failure (a [`BlockError`] cause, such as a capacity refusal or a
//! `code.run` time limit or program failure, or an Oracle queue refusal),
//! halts the session: the Agent executor wakes and ends the invocation at
//! once, dropping calls still in flight, and the session's `before_model`
//! hook ends the loop before the model sees the stop. Nothing here is shared between invocations except
//! the engine-owned [`ToolCallsOwner`] of one step's attempts.

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::{Value, json};
use skald_agent::{BeforeModelFn, CallbackOutcome};
use skald_tool::{AgentTool, StructuredInvocationError, ToolError, ToolRegistry};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};
use tokio::time::Instant;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::agent::BuiltinTool;
use wyrd_spec::error::WyrdError;
use wyrd_spec::vala::error::BifrostError;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::{Producer, ToolCallRecord, ToolRecipient};

use super::{BlockError, CallInput};

/// Binds declared built-ins to one Agent step invocation.
///
/// The server implementation authorizes, materializes the workspace, and
/// builds tools carrying the invocation's original-subject authority. An
/// offline host without built-ins has no toolset, so an Agent declaring
/// tools fails rather than running without them.
#[async_trait]
pub trait AgentToolset: Send + Sync {
    /// Open the tool session for `tools` of the Agent call `id`, in its
    /// resolved `workspace` and cancelled at `deadline`. `producer` is the
    /// step attempt a `publish` call retains content for; a check has none.
    ///
    /// Called once per Agent step or check attempt, before the first model
    /// call. The returned session lives exactly as long as that attempt.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] when a tool needs a workspace the call
    /// does not declare or the workspace cannot be materialized within its
    /// limits, [`BlockError::Retryable`] for a transient evidence read, and
    /// [`BlockError::Busy`] when the process has no workspace to admit.
    async fn open(
        &self,
        id: &TaskId,
        workspace: &BTreeMap<String, CallInput>,
        deadline: Instant,
        tools: &[BuiltinTool],
        producer: Option<&Producer>,
    ) -> Result<ToolSession, BlockError>;
}

/// Bounds every call of one tool session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolLimits {
    /// Largest compact JSON result delivered to a recipient, in bytes.
    pub max_result_bytes: usize,
    /// Largest compact JSON argument object of one call, in bytes.
    pub max_argument_bytes: usize,
    /// Most direct and nested requests of one logical invocation.
    pub max_calls: u32,
}

/// Runs `code.run` with nested tool calls.
///
/// The host implements it for its sandbox; the session hands each run a
/// [`ToolBridge`] through which the program's `call_tool` requests take the
/// same path as the model's direct calls.
#[async_trait]
pub trait CodeTool: Send + Sync {
    /// Run the `code.run` arguments `args`, sending nested requests to
    /// `bridge`.
    ///
    /// # Errors
    /// Returns the tool failure the model sees; a failure whose cause is a
    /// [`BlockError`] halts the session with that error instead, so a time
    /// limit or a program failure ends the invocation.
    async fn run(&self, args: Value, bridge: ToolBridge) -> Result<Value, ToolError>;
}

/// The caller-owned tool-call records, request allowance, and host-work
/// settlement of one logical invocation: a step action across its retries,
/// or one check.
///
/// The engine creates one per invocation, keeps it outside every attempt it
/// times, and hands each attempt only a [`ToolCalls`] handle in
/// [`BlockCall::tool_calls`](super::BlockCall). After each attempt, even one
/// it timed out and dropped, it awaits [`Self::settle`] so host work the
/// attempt started ends before a retry or result, then reads
/// [`Self::settled`] into the produced result whatever the outcome. It is the
/// only way to await or read that state, so an executor cannot take the
/// cleanup its caller must finish.
#[derive(Debug)]
pub struct ToolCallsOwner {
    /// The state its handles share.
    calls: ToolCalls,
}

impl Default for ToolCallsOwner {
    /// A fresh invocation: no records, nothing spent, nothing to settle.
    fn default() -> Self {
        Self {
            calls: ToolCalls {
                inner: Arc::default(),
            },
        }
    }
}

impl ToolCallsOwner {
    /// The invocation as an earlier run of its execution left it: fresh when
    /// it recorded no attempt (`recorded` is zero), otherwise with its whole
    /// allowance spent.
    ///
    /// A recorded attempt without a settlement never reported its spend, so
    /// it counts as having spent everything; a takeover never replenishes
    /// the allowance an interrupted attempt may have used.
    #[must_use]
    pub fn resumed(recorded: u32) -> Self {
        let owner = Self::default();
        if recorded > 0 {
            owner.calls.inner.spent.store(u32::MAX, Ordering::SeqCst);
        }
        owner
    }

    /// A handle an attempt records into, spends from, and registers its host
    /// work with; it cannot await or read that state.
    #[must_use]
    pub fn handle(&self) -> ToolCalls {
        self.calls.clone()
    }

    /// Every admitted call in start order.
    ///
    /// A call still in flight when its attempt ended keeps its arguments and
    /// recipient but returned nothing, so it has no digest, zero bytes, and a
    /// cancellation error.
    #[must_use]
    pub fn settled(&self) -> Vec<ToolCallRecord> {
        self.calls
            .lock()
            .iter()
            .map(|(record, finished)| {
                let mut record = record.clone();
                if !finished {
                    record.output_sha256 = None;
                    record.returned_bytes = 0;
                    record.error =
                        Some("cancelled: the invocation ended before the call returned".to_owned());
                }
                record
            })
            .collect()
    }

    /// Wait until the host work every attempt's session started has ended.
    ///
    /// A session registers its settlement when it opens, so this still
    /// waits after the attempt that owned the session was dropped, as when
    /// the engine timed it out.
    pub async fn settle(&self) {
        loop {
            let pending = self
                .calls
                .inner
                .settlements
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .pop();
            match pending {
                Some(settlement) => settlement.await,
                None => return,
            }
        }
    }
}

/// An attempt's handle on its invocation's [`ToolCallsOwner`].
///
/// Tool calls spend its allowance and record into it, and a session
/// registers its host-work settlement with it; only the owner awaits that
/// settlement or reads the records. A clone shares the same state.
#[derive(Clone)]
pub struct ToolCalls {
    /// Shared state.
    inner: Arc<CallLog>,
}

/// The state behind [`ToolCallsOwner`] and its [`ToolCalls`] handles.
#[derive(Default)]
struct CallLog {
    /// Requests spent; never refunded, and above any allowance once
    /// [`ToolCallsOwner::resumed`] counts an interrupted attempt.
    spent: AtomicU32,
    /// Admitted calls in start order, each with whether it finished.
    records: Mutex<Vec<(ToolCallRecord, bool)>>,
    /// Settlements of attempts' sessions not yet awaited.
    settlements: Mutex<Vec<Settlement>>,
}

/// Resolves once the host work one session's calls started has ended.
type Settlement = Pin<Box<dyn Future<Output = ()> + Send>>;

impl fmt::Debug for ToolCalls {
    /// The spent count and record count, never arguments.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ToolCalls")
            .field("spent", &self.inner.spent.load(Ordering::SeqCst))
            .field("records", &self.lock().len())
            .finish()
    }
}

impl ToolCalls {
    /// Keep `settlement` for the owner's next [`ToolCallsOwner::settle`].
    fn defer(&self, settlement: Settlement) {
        self.inner
            .settlements
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(settlement);
    }

    /// Spend one request of `limit`, or `false` once all are spent.
    fn spend(&self, limit: u32) -> bool {
        self.inner
            .spent
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |spent| {
                (spent < limit).then(|| spent + 1)
            })
            .is_ok()
    }

    /// Record an admitted call's start, returning its slot.
    fn start(&self, record: ToolCallRecord) -> usize {
        let mut records = self.lock();
        records.push((record, false));
        records.len() - 1
    }

    /// Replace the call at `slot` with its finished `record`.
    ///
    /// # Panics
    /// Panics when `slot` was not returned by [`Self::start`] on these
    /// records, which only ever grow.
    fn finish(&self, slot: usize, record: ToolCallRecord) {
        self.lock()[slot] = (record, true);
    }

    /// The records, surviving a poisoned lock.
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<(ToolCallRecord, bool)>> {
        self.inner
            .records
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// The tools of one attempt and the resources they hold.
pub struct ToolSession {
    /// Executable tools keyed by built-in wire name.
    registry: ToolRegistry,
    /// What runs `code.run` with a bridge; without one `code.run` runs as
    /// its registry tool, computation only.
    code: Option<Arc<dyn CodeTool>>,
    /// Bounds of every call.
    limits: ToolLimits,
    /// Released when the session drops, such as the attempt workspace.
    _guard: Box<dyn Any + Send + Sync>,
    /// Resolves once host work the session's calls started has ended; it
    /// moves to the invocation's [`ToolCalls`] when the session opens. In a
    /// mutex only so the session is `Sync`.
    settled: Mutex<Option<Settlement>>,
    /// Raised by the first request that stops the session.
    halt: Halt,
}

impl ToolSession {
    /// A session over `registry` bounded by `limits`, holding `guard` until
    /// it drops.
    #[must_use]
    pub fn new(
        registry: ToolRegistry,
        limits: ToolLimits,
        guard: Box<dyn Any + Send + Sync>,
    ) -> Self {
        Self {
            registry,
            code: None,
            limits,
            _guard: guard,
            settled: Mutex::new(None),
            halt: Halt::default(),
        }
    }

    /// Wait for `settled` before the attempt ends, so blocking host work a
    /// dropped call started, such as an extraction, ends before the
    /// attempt's resources are released and before the engine retries the
    /// step or produces its result, even when it timed the attempt out.
    #[must_use]
    pub fn with_settlement(mut self, settled: impl Future<Output = ()> + Send + 'static) -> Self {
        self.settled = Mutex::new(Some(Box::pin(settled)));
        self
    }

    /// Register the session's settlement with `calls`, whose owner awaits
    /// it after the attempt however it ended.
    pub(crate) fn register_with(&self, calls: &ToolCalls) {
        let settled = self
            .settled
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(settled) = settled {
            calls.defer(settled);
        }
    }

    /// Run `code.run` through `code`, which receives a [`ToolBridge`].
    #[must_use]
    pub fn with_code(mut self, code: Arc<dyn CodeTool>) -> Self {
        self.code = Some(code);
        self
    }

    /// Why the session stopped, if a request stopped it.
    ///
    /// The Agent executor reads it once the loop ends: a raised halt decides
    /// the invocation's error whatever the loop returned: the host's
    /// [`BlockError`], such as [`BlockError::Busy`] for a capacity refusal,
    /// or a terminal `WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED` for a
    /// spent allowance.
    pub(crate) fn halted(&self) -> Option<BlockError> {
        self.halt.stop.borrow().as_ref().map(Stop::error)
    }

    /// Resolves once a request stops this session, immediately when one
    /// already did.
    ///
    /// The Agent executor races the loop against it, so a stop ends the
    /// invocation while sibling tool calls are still pending; dropping the
    /// loop drops those calls.
    pub(crate) async fn halt_raised(&self) {
        self.halt.raised().await;
    }

    /// A `before_model` hook that aborts the Agent loop once the session
    /// stopped, so the stop never reaches the model and no further model
    /// call is made.
    pub(crate) fn halt_before_model(&self) -> BeforeModelFn {
        let halt = self.halt.clone();
        Arc::new(move |_, _| match halt.stop.borrow().as_ref() {
            Some(stop) => CallbackOutcome::Abort(WyrdError::internal(format!(
                "the tool session stopped: {}",
                stop.error()
            ))),
            None => CallbackOutcome::Continue,
        })
    }

    /// Resolve every declared built-in into a recording tool for the loop.
    ///
    /// Resolution goes through the Skald [`ToolRegistry`]. The returned tools
    /// and any [`ToolBridge`] share `calls` and at most `concurrency` active
    /// operations other than `code.run`, which holds no permit so its nested
    /// calls can always acquire one.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] when the registry lacks a declared
    /// tool, so a tool is never silently omitted.
    pub(crate) fn resolve(
        &self,
        tools: &[BuiltinTool],
        calls: &ToolCalls,
        concurrency: usize,
    ) -> Result<Vec<Arc<dyn AgentTool>>, BlockError> {
        let declared = tools
            .iter()
            .map(|&tool| {
                self.registry
                    .resolve(tool.as_str())
                    .map(|inner| (tool, inner))
                    .map_err(|error| BlockError::Terminal {
                        reason: format!("tool {}: {error}", tool.as_str()),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let dispatch = Arc::new(Dispatch {
            declared,
            code: self.code.clone(),
            limits: self.limits,
            calls: calls.clone(),
            halt: self.halt.clone(),
            permits: Arc::new(Semaphore::new(concurrency.max(1))),
        });
        Ok(tools
            .iter()
            .map(|&tool| {
                Arc::new(RecordedTool {
                    tool,
                    name: provider_tool_name(tool),
                    dispatch: Arc::clone(&dispatch),
                }) as Arc<dyn AgentTool>
            })
            .collect())
    }
}

/// The provider-facing name of `tool`.
///
/// Provider function names allow only letters, digits, `_`, and `-`, so the
/// dotted wire name is spelled with underscores toward the model.
#[must_use]
pub fn provider_tool_name(tool: BuiltinTool) -> String {
    tool.as_str().replace('.', "_")
}

/// A Wyrd error as a structured tool failure keeping its stable problem.
#[must_use]
pub fn tool_error(error: &WyrdError) -> ToolError {
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

/// Why a session stopped.
#[derive(Debug, Clone)]
enum Stop {
    /// A call failed with the host's own [`BlockError`]: process capacity
    /// was refused, or `code.run` hit a time limit or its program failed.
    Block(BlockError),
    /// A request came after the allowance was spent.
    Exhausted(u32),
}

impl Stop {
    /// The invocation error this stop decides.
    fn error(&self) -> BlockError {
        match self {
            Self::Block(error) => error.clone(),
            Self::Exhausted(limit) => {
                let error = exhausted(*limit);
                BlockError::Terminal {
                    reason: format!("{}: {}", error.code(), error.problem().detail),
                }
            }
        }
    }
}

/// The catalog error of a request past a `limit`-request allowance.
fn exhausted(limit: u32) -> WyrdError {
    WyrdError::VerificationToolBudgetExceeded {
        message: format!("the invocation already made its {limit} allowed tool requests"),
        details: json!({"max_calls": limit}),
    }
}

/// One session's stop: the first cause wins and stays raised.
#[derive(Clone)]
struct Halt {
    /// The first stop; unset until one is raised. A watch channel so the
    /// executor can wait for it without missing a stop raised before it
    /// subscribed.
    stop: Arc<watch::Sender<Option<Stop>>>,
}

impl Default for Halt {
    /// An unraised halt.
    fn default() -> Self {
        Self {
            stop: Arc::new(watch::channel(None).0),
        }
    }
}

impl Halt {
    /// Raise `stop` unless an earlier stop is already raised.
    fn raise(&self, stop: Stop) {
        self.stop.send_if_modified(|current| {
            let first = current.is_none();
            if first {
                *current = Some(stop);
            }
            first
        });
    }

    /// Whether a stop is raised.
    fn is_raised(&self) -> bool {
        self.stop.borrow().is_some()
    }

    /// Resolves once a stop is raised.
    async fn raised(&self) {
        let mut raised = self.stop.subscribe();
        // The sender lives in `self`, so the wait cannot observe a closed
        // channel; an error would only mean no stop can ever be raised.
        if raised.wait_for(Option::is_some).await.is_err() {
            std::future::pending::<()>().await;
        }
    }

    /// Raise a stop when `error` is the host's own failure rather than the
    /// recipient's to see: a [`BlockError`] cause, which a host attaches to
    /// a call refused process capacity and to a `code.run` stopped by a time
    /// limit or failed by its program, or the structured
    /// `WYRD_VALA_429_QUERY_QUEUE_FULL` problem of a query refused a place
    /// in this process's Oracle queue, which stops it busy.
    fn observe(&self, error: &ToolError) {
        match error {
            ToolError::Invocation {
                cause: Some(cause), ..
            } => {
                if let Some(error) = cause.downcast_ref::<BlockError>() {
                    self.raise(Stop::Block(error.clone()));
                }
            }
            ToolError::StructuredInvocation(problem)
                if problem.code == BifrostError::QueryQueueFull.code() =>
            {
                self.raise(Stop::Block(BlockError::Busy {
                    reason: format!("{}: {}", problem.code, problem.detail),
                }));
            }
            _ => {}
        }
    }
}

/// What every call of one attempt shares, whichever its origin.
struct Dispatch {
    /// The declared tools' registry entries.
    declared: Vec<(BuiltinTool, Arc<dyn AgentTool>)>,
    /// What runs `code.run` with a bridge.
    code: Option<Arc<dyn CodeTool>>,
    /// Bounds of every call.
    limits: ToolLimits,
    /// The logical invocation's records and allowance.
    calls: ToolCalls,
    /// The session's stop.
    halt: Halt,
    /// Active operations other than `code.run`.
    permits: Arc<Semaphore>,
}

impl Dispatch {
    /// The registry entry of declared `tool`.
    fn declared(&self, tool: BuiltinTool) -> Option<&Arc<dyn AgentTool>> {
        self.declared
            .iter()
            .find_map(|(declared, inner)| (*declared == tool).then_some(inner))
    }

    /// Spend one request of the invocation's allowance.
    ///
    /// # Errors
    /// Returns `WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED` and raises the
    /// session's halt once the allowance is spent, or when the session
    /// already stopped, so no further work starts.
    fn spend(&self) -> Result<(), ToolError> {
        if self.halt.is_raised() {
            return Err(stopped());
        }
        if self.calls.spend(self.limits.max_calls) {
            return Ok(());
        }
        self.halt.raise(Stop::Exhausted(self.limits.max_calls));
        Err(tool_error(&exhausted(self.limits.max_calls)))
    }

    /// Run the admitted `tool` with `args` for `recipient` and record it.
    ///
    /// Arguments must be one JSON object within the argument limit, checked
    /// before anything is recorded. The record then starts, before the call
    /// waits for one of the concurrency permits every operation except
    /// `code.run` holds, so a call cancelled while waiting is still recorded.
    /// A session stopped before the permit is granted ends the wait, and one
    /// stopped while it was granted invokes nothing; either way the record
    /// stays unfinished and settles as cancelled. A result larger than the
    /// result limit is replaced by an error, so the recipient never receives
    /// it and the record shows no returned bytes. A failed call records its
    /// error prefixed with its stable Wyrd code; a host failure also halts
    /// the session (see [`Halt::observe`]). A successful call's record
    /// finishes only when its [`Delivery`] is delivered.
    ///
    /// # Errors
    /// Returns [`ToolError::InvalidInput`] for arguments that are not an
    /// object or exceed the limit, [`ToolError::Invocation`] once the
    /// session stopped and for an oversized or unserializable result, or
    /// the tool's own error.
    ///
    /// # Panics
    /// Panics when `tool` is not declared, which every caller checks first.
    async fn call(
        self: &Arc<Self>,
        tool: BuiltinTool,
        call_id: Option<&str>,
        args: Value,
        recipient: ToolRecipient,
    ) -> Result<Delivery, ToolError> {
        if !args.is_object() {
            return Err(ToolError::InvalidInput(
                "tool arguments must be one JSON object".to_owned(),
            ));
        }
        let max = self.limits.max_argument_bytes;
        if serde_json::to_writer(&mut Capped::new(max), &args).is_err() {
            return Err(ToolError::InvalidInput(format!(
                "the arguments exceed the {max}-byte tool argument limit"
            )));
        }
        let mut record = ToolCallRecord {
            tool,
            arguments: args.clone(),
            recipient,
            output_sha256: None,
            returned_bytes: 0,
            error: None,
        };
        let slot = self.calls.start(record.clone());
        let result = match (&self.code, tool) {
            (Some(code), BuiltinTool::CodeRun) => {
                code.run(
                    args,
                    ToolBridge {
                        dispatch: Arc::clone(self),
                    },
                )
                .await
            }
            (_, BuiltinTool::CodeRun) => self.invoke(tool, call_id, args).await,
            _ => {
                let permit = Mutex::new(Some(self.permit().await?));
                ACTIVE.scope(permit, self.invoke(tool, call_id, args)).await
            }
        };
        let result = result.and_then(|output| {
            // Encoding stops at the limit, so an oversized result never
            // allocates more than the limit.
            let mut writer = Capped::new(self.limits.max_result_bytes);
            match serde_json::to_writer(&mut writer, &output) {
                Ok(()) => {}
                Err(error) if error.is_io() => {
                    return Err(ToolError::Invocation {
                        detail: format!(
                            "the result exceeds the {}-byte tool result limit",
                            self.limits.max_result_bytes
                        ),
                        cause: None,
                    });
                }
                Err(error) => return Err(ToolError::OutputSerialization(error.to_string())),
            }
            Ok((output, writer.bytes))
        });
        match result {
            Ok((value, bytes)) => {
                record.output_sha256 = Some(Sha256Hex::digest(&bytes));
                record.returned_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
                Ok(Delivery {
                    value,
                    calls: self.calls.clone(),
                    slot,
                    record,
                })
            }
            Err(error) => {
                self.halt.observe(&error);
                self.calls.finish(slot, failed(record, &error));
                Err(error)
            }
        }
    }

    /// Invoke declared `tool`'s registry entry with `args`, for provider
    /// call `call_id` when the model made the request.
    ///
    /// # Errors
    /// Returns the tool's own error.
    ///
    /// # Panics
    /// Panics when `tool` is not declared, which every caller checks first.
    async fn invoke(
        &self,
        tool: BuiltinTool,
        call_id: Option<&str>,
        args: Value,
    ) -> Result<Value, ToolError> {
        let inner = self
            .declared(tool)
            .expect("only declared tools are dispatched");
        match call_id {
            Some(call_id) => inner.invoke_call(call_id, args).await,
            None => inner.invoke(args).await,
        }
    }

    /// One concurrency permit, unless the session stops first.
    ///
    /// # Errors
    /// Returns [`ToolError::Invocation`] when the session stopped before or
    /// while the permit was granted, so nothing is invoked after a stop.
    ///
    /// # Panics
    /// Panics when the semaphore is closed, which the session never does.
    async fn permit(&self) -> Result<OwnedSemaphorePermit, ToolError> {
        let permit = tokio::select! {
            permit = Arc::clone(&self.permits).acquire_owned() => {
                permit.expect("the session never closes its permits")
            }
            () = self.halt.raised() => return Err(stopped()),
        };
        if self.halt.is_raised() {
            return Err(stopped());
        }
        Ok(permit)
    }
}

tokio::task_local! {
    /// The concurrency permit of the call whose tool is being invoked.
    static ACTIVE: Mutex<Option<OwnedSemaphorePermit>>;
}

/// Take the concurrency permit of the call now invoking this tool, so the
/// blocking work it starts keeps the permit until that work ends, even when
/// the call is dropped first.
///
/// `None` outside a permit-holding call, such as `code.run`, or once taken.
#[must_use]
pub fn hold_permit() -> Option<OwnedSemaphorePermit> {
    ACTIVE
        .try_with(|permit| permit.lock().unwrap_or_else(PoisonError::into_inner).take())
        .ok()
        .flatten()
}

/// The refusal of a request once the session stopped.
fn stopped() -> ToolError {
    ToolError::Invocation {
        detail: "the tool session stopped".to_owned(),
        cause: None,
    }
}

/// `record` as delivering no value because of `error`, which it keeps
/// prefixed with its stable Wyrd code.
fn failed(mut record: ToolCallRecord, error: &ToolError) -> ToolCallRecord {
    record.output_sha256 = None;
    record.returned_bytes = 0;
    record.error = Some(format!("{}: {error}", WyrdError::from(error).code()));
    record
}

/// A successful call's value, recorded as returned only once delivered.
///
/// The record already carries the value's digest and size, but its call
/// stays unfinished until [`Self::deliver`]: dropping the delivery, as when
/// a program ends before Python receives the value, settles the record as
/// cancelled with no digest, and [`Self::refuse`] records why the value
/// could not be handed over.
pub struct Delivery {
    /// The result.
    value: Value,
    /// The logical invocation's records.
    calls: ToolCalls,
    /// The call's record slot.
    slot: usize,
    /// The finished record.
    record: ToolCallRecord,
}

impl Delivery {
    /// The result about to be delivered.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Record the value as returned to its recipient and hand it over.
    pub fn deliver(self) -> Value {
        self.calls.finish(self.slot, self.record);
        self.value
    }

    /// Record that the value never reached its recipient because of
    /// `error`, such as a value Python cannot represent.
    pub fn refuse(self, error: &ToolError) {
        self.calls.finish(self.slot, failed(self.record, error));
    }
}

/// The nested-call entry `code.run` hands its program.
///
/// Each request spends the invocation's allowance, resolves only a declared
/// built-in other than `code.run`, `fs.write`, or `publish`, and then runs through the same recorded
/// path as a direct call, recorded with recipient `code`.
pub struct ToolBridge {
    /// The attempt's shared dispatch.
    dispatch: Arc<Dispatch>,
}

impl ToolBridge {
    /// Call the declared built-in named `name` with `args`.
    ///
    /// # Errors
    /// Returns `WYRD_VERIFICATION_422_TOOL_BUDGET_EXCEEDED` once the
    /// allowance is spent, `WYRD_AGENT_404_TOOL_NOT_IN_AGENT` for a name
    /// that is not a declared built-in or is one [`Self::resolve`] keeps from
    /// code, and otherwise the
    /// recorded call's errors. Check [`Self::halted`] after any error: a
    /// halted session must not continue. The value counts as returned only
    /// once the [`Delivery`] is delivered to the program.
    pub async fn call(&self, name: &str, args: Value) -> Result<Delivery, ToolError> {
        self.dispatch.spend()?;
        let tool = self.resolve(name)?;
        self.dispatch
            .call(tool, None, args, ToolRecipient::Code)
            .await
    }

    /// Refuse a request to `name` whose arguments could not be read as JSON
    /// for `reason`, spending and resolving it as [`Self::call`] does.
    ///
    /// Returns the budget or unavailable-target error when one applies and
    /// otherwise invalid input; nothing is dispatched or recorded.
    #[must_use]
    pub fn refuse(&self, name: &str, reason: String) -> ToolError {
        match self.dispatch.spend().and_then(|()| self.resolve(name)) {
            Ok(_) => ToolError::InvalidInput(reason),
            Err(error) => error,
        }
    }

    /// The declared built-in `name` names, never `code.run`, `fs.write`, or
    /// `publish`.
    ///
    /// Code already writes its private `/work` directly, and a nested
    /// request has no provider call identity a publication could replay
    /// under, so neither writable tool is reachable from code.
    ///
    /// # Errors
    /// Returns `WYRD_AGENT_404_TOOL_NOT_IN_AGENT` for any other name.
    fn resolve(&self, name: &str) -> Result<BuiltinTool, ToolError> {
        BuiltinTool::from_name(name)
            .filter(|tool| {
                !matches!(
                    tool,
                    BuiltinTool::CodeRun | BuiltinTool::FsWrite | BuiltinTool::Publish
                ) && self.dispatch.declared(*tool).is_some()
            })
            .ok_or_else(|| {
                tool_error(&WyrdError::AgentToolNotInAgent {
                    message: "code may call only the Agent's declared tools other than \
                              code.run, fs.write, and publish"
                        .to_owned(),
                    details: json!({}),
                })
            })
    }

    /// Whether the session stopped, so the program must end without a
    /// result: its allowance is spent or a call was refused capacity.
    #[must_use]
    pub fn halted(&self) -> bool {
        self.dispatch.halt.is_raised()
    }

    /// Largest compact JSON argument object of one request, in bytes.
    #[must_use]
    pub fn max_argument_bytes(&self) -> usize {
        self.dispatch.limits.max_argument_bytes
    }

    /// Largest compact JSON result one request delivers, in bytes.
    #[must_use]
    pub fn max_result_bytes(&self) -> usize {
        self.dispatch.limits.max_result_bytes
    }
}

/// A declared built-in as the model calls it, recorded with recipient
/// `model`.
struct RecordedTool {
    /// The built-in the record names.
    tool: BuiltinTool,
    /// Provider-safe name.
    name: String,
    /// The attempt's shared dispatch.
    dispatch: Arc<Dispatch>,
}

impl RecordedTool {
    /// The registry tool behind this name.
    ///
    /// # Panics
    /// Panics when the tool is not declared, which [`ToolSession::resolve`]
    /// prevents.
    fn inner(&self) -> &Arc<dyn AgentTool> {
        self.dispatch
            .declared(self.tool)
            .expect("every recorded tool is declared")
    }
}

#[async_trait]
impl AgentTool for RecordedTool {
    /// The provider-safe name.
    fn name(&self) -> &str {
        &self.name
    }

    /// The bound tool's description.
    fn description(&self) -> &str {
        self.inner().description()
    }

    /// The bound tool's argument schema.
    fn input_schema(&self) -> Value {
        self.inner().input_schema()
    }

    /// The bound tool's result schema.
    fn output_schema(&self) -> Value {
        self.inner().output_schema()
    }

    /// Spend one request, then run and record the call for the model
    /// outside any provider call.
    ///
    /// # Errors
    /// Returns the spend refusal or [`Dispatch::call`] failures.
    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        self.dispatch.spend()?;
        self.dispatch
            .call(self.tool, None, args, ToolRecipient::Model)
            .await
            .map(Delivery::deliver)
    }

    /// Spend one request, then run and record the model's provider call
    /// `call_id`, which a `publish` call retains its content under.
    ///
    /// # Errors
    /// Returns the spend refusal or [`Dispatch::call`] failures.
    async fn invoke_call(&self, call_id: &str, args: Value) -> Result<Value, ToolError> {
        self.dispatch.spend()?;
        self.dispatch
            .call(self.tool, Some(call_id), args, ToolRecipient::Model)
            .await
            .map(Delivery::deliver)
    }
}

/// Encoded bytes limited to `max`; a write past the limit fails without
/// growing the buffer.
struct Capped {
    /// What fit.
    bytes: Vec<u8>,
    /// Byte limit.
    max: usize,
}

impl Capped {
    /// Empty bytes limited to `max`.
    const fn new(max: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max,
        }
    }
}

impl std::io::Write for Capped {
    /// Append `buf` whole, or fail when it would pass the limit.
    ///
    /// # Errors
    /// Returns an IO error once the limit would be passed.
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.len() > self.max - self.bytes.len() {
            return Err(std::io::Error::other("the tool result limit is reached"));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    /// Nothing is buffered.
    ///
    /// # Errors
    /// Never.
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::ToolCallsOwner;

    /// An invocation an earlier run never recorded spends its allowance
    /// afresh, while one with a recorded, unsettled attempt starts with the
    /// whole allowance spent, so a takeover never replenishes it.
    ///
    /// # Panics
    /// Panics when a resumed allowance admits a request or a fresh one
    /// refuses its first.
    #[test]
    fn resumed_invocation_counts_an_interrupted_attempt_as_fully_spent() {
        let fresh = ToolCallsOwner::resumed(0).handle();
        assert!(fresh.spend(1));
        assert!(!fresh.spend(1), "the allowance holds one request");
        let resumed = ToolCallsOwner::resumed(1).handle();
        assert!(!resumed.spend(u32::MAX - 1));
        assert!(!resumed.spend(128));
    }
}
