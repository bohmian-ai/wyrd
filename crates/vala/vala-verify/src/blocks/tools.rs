//! The tool seam an Agent step action or check uses.
//!
//! The host binds executable built-ins to one invocation's authority,
//! deadline, and workspace and hands them over as a [`ToolSession`]: a Skald
//! [`ToolRegistry`] holding only the tools the Agent declared, plus a guard
//! that releases the invocation's resources when dropped. The Agent executor
//! resolves each declared name through the registry, wraps every tool in a
//! [`RecordedTool`] that records the call for provenance, and attaches the
//! tools to the ordinary Skald Agent loop. A tool whose failure's cause is
//! [`BlockError::Busy`] raises the session's refusal, which wakes the Agent
//! executor so the invocation fails busy at once, dropping sibling tool calls
//! still in flight; the session's `before_model` hook also ends the loop
//! before the model sees the refusal. Nothing here is shared between
//! invocations.

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::Value;
use skald_agent::{BeforeModelFn, CallbackOutcome};
use skald_tool::{AgentTool, ToolError, ToolRegistry};
use tokio::sync::watch;
use tokio::time::Instant;
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::agent::BuiltinTool;
use wyrd_spec::verification::TaskId;
use wyrd_spec::verification::execution::ToolCallRecord;

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
    /// resolved `workspace` and cancelled at `deadline`.
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
    ) -> Result<ToolSession, BlockError>;
}

/// The tools of one invocation and the resources they hold.
pub struct ToolSession {
    /// Executable tools keyed by built-in wire name.
    registry: ToolRegistry,
    /// Largest serialized result returned to the model, in bytes.
    max_result_bytes: usize,
    /// Released when the session drops, such as the invocation workspace.
    _guard: Box<dyn Any + Send + Sync>,
    /// Raised by the first tool refused process capacity.
    refusal: Refusal,
}

impl ToolSession {
    /// A session over `registry`, refusing results above `max_result_bytes`
    /// and holding `guard` until it drops.
    #[must_use]
    pub fn new(
        registry: ToolRegistry,
        max_result_bytes: usize,
        guard: Box<dyn Any + Send + Sync>,
    ) -> Self {
        Self {
            registry,
            max_result_bytes,
            _guard: guard,
            refusal: Refusal::default(),
        }
    }

    /// The reason of the first tool call refused process capacity, if any.
    ///
    /// The Agent executor reads it once the loop ends: a raised refusal
    /// makes the invocation [`BlockError::Busy`] whatever the loop returned.
    pub(crate) fn refused(&self) -> Option<String> {
        self.refusal.reason.borrow().clone()
    }

    /// Resolves once any tool of this session is refused process capacity,
    /// immediately when one already was.
    ///
    /// The Agent executor races the loop against it, so a refusal ends the
    /// invocation while sibling tool calls are still pending; dropping the
    /// loop drops those calls. Never resolves for a session without a
    /// refusal.
    pub(crate) async fn refusal_raised(&self) {
        let mut raised = self.refusal.reason.subscribe();
        // The sender lives in `self`, so the wait cannot observe a closed
        // channel; an error would only mean no refusal can ever be raised.
        if raised.wait_for(Option::is_some).await.is_err() {
            std::future::pending::<()>().await;
        }
    }

    /// A `before_model` hook that aborts the Agent loop once any tool of this
    /// session is refused process capacity, so the refusal never reaches the
    /// model and no further model call is made.
    pub(crate) fn halt_on_refusal(&self) -> BeforeModelFn {
        let refusal = self.refusal.clone();
        Arc::new(move |_, _| match refusal.reason.borrow().as_deref() {
            Some(reason) => CallbackOutcome::Abort(wyrd_spec::error::WyrdError::internal(format!(
                "a tool was refused process capacity: {reason}"
            ))),
            None => CallbackOutcome::Continue,
        })
    }

    /// Resolve every declared built-in into a recording tool for the loop.
    ///
    /// Resolution goes through the Skald [`ToolRegistry`]; the returned tools
    /// append to `records` in call order.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] when the registry lacks a declared
    /// tool, so a tool is never silently omitted.
    pub(crate) fn resolve(
        &self,
        tools: &[BuiltinTool],
        records: &Arc<Mutex<Vec<Option<ToolCallRecord>>>>,
    ) -> Result<Vec<Arc<dyn AgentTool>>, BlockError> {
        tools
            .iter()
            .map(|&tool| {
                let inner =
                    self.registry
                        .resolve(tool.as_str())
                        .map_err(|error| BlockError::Terminal {
                            reason: format!("tool {}: {error}", tool.as_str()),
                        })?;
                Ok(Arc::new(RecordedTool {
                    tool,
                    name: provider_tool_name(tool),
                    inner,
                    max_result_bytes: self.max_result_bytes,
                    records: Arc::clone(records),
                    refusal: self.refusal.clone(),
                }) as Arc<dyn AgentTool>)
            })
            .collect()
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

/// One session's capacity refusal: the first reason wins and stays raised.
#[derive(Clone)]
struct Refusal {
    /// The first refused call's bounded reason; unset until one is refused.
    /// A watch channel so the executor can wait for it without missing a
    /// refusal raised before it subscribed.
    reason: Arc<watch::Sender<Option<String>>>,
}

impl Default for Refusal {
    /// An unraised refusal.
    fn default() -> Self {
        Self {
            reason: Arc::new(watch::channel(None).0),
        }
    }
}

impl Refusal {
    /// Raise the refusal when `error`'s cause is [`BlockError::Busy`], the
    /// typed cause a host attaches to a tool refused process capacity; any
    /// other failure is the model's to see.
    fn observe(&self, error: &ToolError) {
        if let ToolError::Invocation {
            cause: Some(cause), ..
        } = error
            && let Some(BlockError::Busy { reason }) = cause.downcast_ref::<BlockError>()
        {
            self.reason.send_if_modified(|current| {
                let first = current.is_none();
                if first {
                    *current = Some(reason.clone());
                }
                first
            });
        }
    }
}

/// A built-in that records each call as a [`ToolCallRecord`].
struct RecordedTool {
    /// The built-in the record names.
    tool: BuiltinTool,
    /// Provider-safe name.
    name: String,
    /// The bound tool.
    inner: Arc<dyn AgentTool>,
    /// Largest serialized result returned to the model.
    max_result_bytes: usize,
    /// The invocation's records; a slot is reserved when a call starts so the
    /// list keeps call order under concurrent tool calls.
    records: Arc<Mutex<Vec<Option<ToolCallRecord>>>>,
    /// The session's capacity refusal this tool raises.
    refusal: Refusal,
}

#[async_trait]
impl AgentTool for RecordedTool {
    /// The provider-safe name.
    fn name(&self) -> &str {
        &self.name
    }

    /// The bound tool's description.
    fn description(&self) -> &str {
        self.inner.description()
    }

    /// The bound tool's argument schema.
    fn input_schema(&self) -> Value {
        self.inner.input_schema()
    }

    /// The bound tool's result schema.
    fn output_schema(&self) -> Value {
        self.inner.output_schema()
    }

    /// Invoke the bound tool and record arguments, output digest, and size.
    ///
    /// A failed call records its error prefixed with its stable Wyrd code, so
    /// a denied call names `WYRD_PERMISSION_403_DENIED_RBAC`. A result larger than the session limit is replaced by an error, so
    /// the model never receives it and the record shows no returned bytes.
    /// A failure caused by [`BlockError::Busy`] also raises the session's
    /// refusal.
    ///
    /// # Errors
    /// Returns the bound tool's error, or [`ToolError::Invocation`] for an
    /// oversized or unserializable result.
    async fn invoke(&self, args: Value) -> Result<Value, ToolError> {
        let slot = {
            let mut records = self.records.lock().unwrap_or_else(PoisonError::into_inner);
            records.push(None);
            records.len() - 1
        };
        let mut record = ToolCallRecord {
            tool: self.tool,
            arguments: args.clone(),
            output_sha256: None,
            returned_bytes: 0,
            error: None,
        };
        let result = self.inner.invoke(args).await.and_then(|output| {
            // Encoding stops at the limit, so an oversized result never
            // allocates more than the limit.
            let mut writer = Capped {
                bytes: Vec::new(),
                max: self.max_result_bytes,
            };
            match serde_json::to_writer(&mut writer, &output) {
                Ok(()) => {}
                Err(error) if error.is_io() => {
                    return Err(ToolError::Invocation {
                        detail: format!(
                            "the result exceeds the {}-byte tool result limit",
                            self.max_result_bytes
                        ),
                        cause: None,
                    });
                }
                Err(error) => return Err(ToolError::OutputSerialization(error.to_string())),
            }
            let bytes = writer.bytes;
            record.output_sha256 = Some(Sha256Hex::digest(&bytes));
            record.returned_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            Ok(output)
        });
        if let Err(error) = &result {
            self.refusal.observe(error);
            let code = wyrd_spec::error::WyrdError::from(error).code();
            record.error = Some(format!("{code}: {error}"));
        }
        self.records.lock().unwrap_or_else(PoisonError::into_inner)[slot] = Some(record);
        result
    }
}

/// The finished records of an invocation, dropping calls still in flight
/// when the loop ended.
pub(crate) fn finished(records: &Mutex<Vec<Option<ToolCallRecord>>>) -> Vec<ToolCallRecord> {
    records
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .flatten()
        .cloned()
        .collect()
}

/// Encoded bytes limited to `max`; a write past the limit fails without
/// growing the buffer.
struct Capped {
    /// What fit.
    bytes: Vec<u8>,
    /// Byte limit.
    max: usize,
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
