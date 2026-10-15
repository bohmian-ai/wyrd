//! The tool seam an Agent step action uses.
//!
//! The host binds executable built-ins to one invocation's authority,
//! deadline, and workspace and hands them over as a [`ToolSession`]: a Skald
//! [`ToolRegistry`] holding only the tools the Agent declared, plus a guard
//! that releases the invocation's resources when dropped. The Agent executor
//! resolves each declared name through the registry, wraps every tool in a
//! [`RecordedTool`] that records the call for provenance, and attaches the
//! tools to the ordinary Skald Agent loop. Nothing here is shared between
//! invocations.

use std::any::Any;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use serde_json::Value;
use skald_tool::{AgentTool, ToolError, ToolRegistry};
use wyrd_spec::auth::Sha256Hex;
use wyrd_spec::card::agent::BuiltinTool;
use wyrd_spec::card::blocks::Action;
use wyrd_spec::verification::execution::ToolCallRecord;

use super::{BlockCall, BlockError};

/// Binds declared built-ins to one Agent step invocation.
///
/// The server implementation authorizes, materializes the workspace, and
/// builds tools carrying the invocation's original-subject authority. An
/// offline host without built-ins has no toolset, so an Agent declaring
/// tools fails rather than running without them.
#[async_trait]
pub trait AgentToolset: Send + Sync {
    /// Open the tool session for `tools` in `call`'s workspace.
    ///
    /// Called once per Agent step attempt, before the first model call. The
    /// returned session lives exactly as long as that attempt.
    ///
    /// # Errors
    /// Returns [`BlockError::Terminal`] when a tool needs a workspace the call
    /// does not declare or the workspace cannot be materialized within its
    /// limits, and [`BlockError::Retryable`] for a transient evidence read.
    async fn open(
        &self,
        call: &BlockCall<'_, Action>,
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
        }
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
    /// A result larger than the session limit is replaced by an error, so
    /// the model never receives it and the record shows no returned bytes.
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
            let bytes = serde_json::to_vec(&output)
                .map_err(|error| ToolError::OutputSerialization(error.to_string()))?;
            if bytes.len() > self.max_result_bytes {
                return Err(ToolError::Invocation {
                    detail: format!(
                        "result of {} bytes exceeds the {}-byte tool result limit",
                        bytes.len(),
                        self.max_result_bytes
                    ),
                    cause: None,
                });
            }
            record.output_sha256 = Some(Sha256Hex::digest(&bytes));
            record.returned_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
            Ok(output)
        });
        if let Err(error) = &result {
            record.error = Some(error.to_string());
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
