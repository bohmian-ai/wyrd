//! Agent run observer trait and no-op default.

use std::time::Duration;

use async_trait::async_trait;

/// Best-effort observer for live agent and workflow run events.
///
/// All methods have default no-op implementations. Implementors only override
/// the methods they care about.
///
/// # Thread Safety
///
/// Observer methods may be called concurrently from parallel workflow steps.
/// Each concurrent agent run has a unique `run_id`, so calls for different
/// runs never share the same key. Implementors that maintain shared mutable
/// state must protect it with appropriate synchronization.
///
/// # Error Handling
///
/// Observer methods return `()`. Panics inside observer methods are caught
/// by the runtime and logged. Never propagate errors from observer code into
/// the agent loop.
#[async_trait]
pub trait Observer: Send + Sync {
    /// Agent run started.
    async fn on_agent_start(
        &self,
        _run_id: &str,
        _parent_run_id: Option<&str>,
        _agent_id: &str,
        _input: &str,
        _session_id: Option<&str>,
    ) {
    }

    /// Loop iteration started.
    async fn on_iteration(&self, _run_id: &str, _agent_id: &str, _index: u32) {}

    /// Provider model call started.
    async fn on_model_call(
        &self,
        _run_id: &str,
        _agent_id: &str,
        _iteration: u32,
        _provider: &str,
        _model: &str,
        _request: &skald_spec::ProviderRequest,
    ) {
    }

    /// Provider model call completed.
    async fn on_model_result(
        &self,
        _run_id: &str,
        _agent_id: &str,
        _iteration: u32,
        _finish_reason: &str,
        _synthetic: bool,
        _response: &skald_spec::ProviderResponse,
    ) {
    }

    /// Tool invocation started.
    async fn on_tool_call(
        &self,
        _run_id: &str,
        _agent_id: &str,
        _iteration: u32,
        _call_id: &str,
        _tool_name: &str,
    ) {
    }

    /// Tool invocation completed.
    async fn on_tool_result(
        &self,
        _run_id: &str,
        _agent_id: &str,
        _iteration: u32,
        _call_id: &str,
        _ok: bool,
    ) {
    }

    /// Agent run finished successfully.
    async fn on_agent_finish(
        &self,
        _run_id: &str,
        _agent_id: &str,
        _finish_reason: &str,
        _iterations: u32,
        _duration: Duration,
    ) {
    }

    /// Agent run failed.
    async fn on_agent_error(&self, _run_id: &str, _agent_id: &str, _code: &str, _message: &str) {}

    /// Workflow run started.
    async fn on_workflow_start(&self, _run_id: &str, _workflow_id: &str, _step_count: usize) {}

    /// Workflow run finished.
    async fn on_workflow_finish(&self, _run_id: &str, _workflow_id: &str, _duration: Duration) {}
}

/// Observer implementation that drops every event.
pub struct NoopObserver;

#[async_trait]
impl Observer for NoopObserver {}
