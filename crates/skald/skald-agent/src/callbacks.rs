//! Callback type aliases and context snapshots for agent runs.

use std::any::Any;
use std::sync::Arc;

use skald_spec::{ProviderRequest, ProviderResponse};
use skald_tool::{AgentTool, ToolError};
use wyrd_spec::error::WyrdError;

use crate::conversation::Conversation;
use crate::error::{AgentError, AgentResult};

/// Outcome a callback can return to control its associated operation.
pub enum CallbackOutcome<T> {
    /// Let the original operation run.
    Continue,
    /// Replace the value the original operation would have returned.
    ReplaceWith(T),
    /// Stop the operation with this error as the cause.
    ///
    /// From an agent or model hook it ends the run
    /// [`FinishReason::CallbackAborted`](crate::FinishReason::CallbackAborted)
    /// with the error on `AgentRun.error`; from a tool hook it reports that one
    /// tool call to the model as failed with the error and the run continues.
    Abort(WyrdError),
}

/// Result of applying a callback chain in registration order.
pub(crate) enum ChainResult<T> {
    /// A callback aborted the current operation, carrying its error and the
    /// value the chain held when it aborted.
    Abort(WyrdError, T),
    /// Effective current value after all callbacks continue or replace.
    Replaced(T),
}

/// Snapshot passed to every agent callback.
#[derive(Clone)]
pub struct AgentContext {
    /// Stable agent id.
    pub agent_id: String,
    /// Optional session id for the current run.
    pub session_id: Option<String>,
    /// Current loop iteration.
    pub iteration: u32,
    /// Read-only conversation snapshot for the callback fire point.
    pub conversation: Arc<Conversation>,
}

/// Callback fired before the agent run begins.
pub type BeforeAgentFn = Arc<dyn Fn(&AgentContext, &str) -> CallbackOutcome<String> + Send + Sync>;

/// Callback fired after the agent run completes.
pub type AfterAgentFn = Arc<
    dyn Fn(&AgentContext, &crate::run::AgentRun) -> CallbackOutcome<crate::run::AgentRun>
        + Send
        + Sync,
>;

/// Callback fired before one provider request is sent.
pub type BeforeModelFn =
    Arc<dyn Fn(&AgentContext, &ProviderRequest) -> CallbackOutcome<ProviderRequest> + Send + Sync>;

/// Callback fired after one provider response is received.
pub type AfterModelFn = Arc<
    dyn Fn(&AgentContext, &ProviderResponse) -> CallbackOutcome<ProviderResponse> + Send + Sync,
>;

/// Callback fired before one tool invocation.
pub type BeforeToolFn = Arc<
    dyn Fn(&AgentContext, &dyn AgentTool, &serde_json::Value) -> CallbackOutcome<serde_json::Value>
        + Send
        + Sync,
>;

/// Callback fired after one tool invocation.
pub type AfterToolFn = Arc<
    dyn Fn(
            &AgentContext,
            &dyn AgentTool,
            &Result<serde_json::Value, skald_tool::ToolError>,
        ) -> CallbackOutcome<Result<serde_json::Value, skald_tool::ToolError>>
        + Send
        + Sync,
>;

/// Applies a generic `(context, value)` callback chain such as the model hooks.
///
/// Each callback runs in registration order under `catch_unwind`; a
/// replacement becomes the value seen by the next callback, and the first
/// abort stops the chain and returns the value held at that point.
///
/// # Errors
///
/// Returns [`AgentError::CallbackPanic`] naming `hook` when a callback panics.
pub(crate) fn apply_chain_with_panic_catch<T, F>(
    chain: &[Arc<F>],
    ctx: &AgentContext,
    value: T,
    hook: &'static str,
) -> AgentResult<ChainResult<T>>
where
    F: Fn(&AgentContext, &T) -> CallbackOutcome<T> + ?Sized,
{
    let mut current = value;
    for callback in chain {
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(ctx, &current)));
        match outcome {
            Ok(CallbackOutcome::Continue) => {}
            Ok(CallbackOutcome::ReplaceWith(replacement)) => current = replacement,
            Ok(CallbackOutcome::Abort(error)) => return Ok(ChainResult::Abort(error, current)),
            Err(payload) => return Err(callback_panic(hook, payload)),
        }
    }
    Ok(ChainResult::Replaced(current))
}

/// Applies the `before_agent` chain to the run's input text.
///
/// Each callback runs in registration order under `catch_unwind`; a
/// replacement becomes the value seen by the next callback, and the first
/// abort stops the chain and returns the value held at that point.
///
/// # Errors
///
/// Returns [`AgentError::CallbackPanic`] naming `hook` when a callback panics.
pub(crate) fn apply_before_agent_chain_with_panic_catch(
    chain: &[BeforeAgentFn],
    ctx: &AgentContext,
    value: String,
    hook: &'static str,
) -> AgentResult<ChainResult<String>> {
    let mut current = value;
    for callback in chain {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback(ctx, current.as_str())
        }));
        match outcome {
            Ok(CallbackOutcome::Continue) => {}
            Ok(CallbackOutcome::ReplaceWith(replacement)) => current = replacement,
            Ok(CallbackOutcome::Abort(error)) => return Ok(ChainResult::Abort(error, current)),
            Err(payload) => return Err(callback_panic(hook, payload)),
        }
    }
    Ok(ChainResult::Replaced(current))
}

/// Applies the `before_tool` chain to one tool call's arguments.
///
/// Each callback runs in registration order under `catch_unwind`; a
/// replacement becomes the value seen by the next callback, and the first
/// abort stops the chain and returns the value held at that point.
///
/// # Errors
///
/// Returns [`AgentError::CallbackPanic`] naming `hook` when a callback panics.
pub(crate) fn apply_chain_with_panic_catch_tool(
    chain: &[BeforeToolFn],
    ctx: &AgentContext,
    tool: &dyn AgentTool,
    args: serde_json::Value,
    hook: &'static str,
) -> AgentResult<ChainResult<serde_json::Value>> {
    let mut current = args;
    for callback in chain {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback(ctx, tool, &current)
        }));
        match outcome {
            Ok(CallbackOutcome::Continue) => {}
            Ok(CallbackOutcome::ReplaceWith(replacement)) => current = replacement,
            Ok(CallbackOutcome::Abort(error)) => return Ok(ChainResult::Abort(error, current)),
            Err(payload) => return Err(callback_panic(hook, payload)),
        }
    }
    Ok(ChainResult::Replaced(current))
}

/// Applies the `after_tool` chain to one tool call's result.
///
/// Each callback runs in registration order under `catch_unwind`; a
/// replacement becomes the value seen by the next callback, and the first
/// abort stops the chain and returns the value held at that point.
///
/// # Errors
///
/// Returns [`AgentError::CallbackPanic`] naming `hook` when a callback panics.
pub(crate) fn apply_chain_with_panic_catch_tool_result(
    chain: &[AfterToolFn],
    ctx: &AgentContext,
    tool: &dyn AgentTool,
    result: Result<serde_json::Value, ToolError>,
    hook: &'static str,
) -> AgentResult<ChainResult<Result<serde_json::Value, ToolError>>> {
    let mut current = result;
    for callback in chain {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            callback(ctx, tool, &current)
        }));
        match outcome {
            Ok(CallbackOutcome::Continue) => {}
            Ok(CallbackOutcome::ReplaceWith(replacement)) => current = replacement,
            Ok(CallbackOutcome::Abort(error)) => return Ok(ChainResult::Abort(error, current)),
            Err(payload) => return Err(callback_panic(hook, payload)),
        }
    }
    Ok(ChainResult::Replaced(current))
}

fn callback_panic(hook: &'static str, payload: Box<dyn Any + Send>) -> AgentError {
    AgentError::CallbackPanic {
        hook: hook.to_owned(),
        payload: extract_panic_message(payload),
    }
}

fn extract_panic_message(payload: Box<dyn Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_owned()
    } else {
        "<non-string panic payload>".to_owned()
    }
}
