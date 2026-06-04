//! Agent run observer trait and no-op default.

use std::time::Duration;

use async_trait::async_trait;

/// Best-effort observer for live agent run events.
///
/// Observer events mirror [`crate::JournalEvent`] data, but observer methods
/// return `()` so observability failures do not alter agent execution.
#[async_trait]
pub trait Observer: Send + Sync {
    /// Agent run started.
    async fn on_agent_start(&self, _agent_id: &str, _input: &str, _session_id: Option<&str>) {}

    /// Loop iteration started.
    async fn on_iteration(&self, _agent_id: &str, _index: u32) {}

    /// Provider model call started.
    async fn on_model_call(&self, _agent_id: &str, _iteration: u32, _provider: &str, _model: &str) {
    }

    /// Provider model call completed.
    async fn on_model_result(
        &self,
        _agent_id: &str,
        _iteration: u32,
        _finish_reason: &str,
        _synthetic: bool,
    ) {
    }

    /// Tool invocation started.
    async fn on_tool_call(
        &self,
        _agent_id: &str,
        _iteration: u32,
        _call_id: &str,
        _tool_name: &str,
    ) {
    }

    /// Tool invocation completed.
    async fn on_tool_result(&self, _agent_id: &str, _iteration: u32, _call_id: &str, _ok: bool) {}

    /// Agent run finished.
    async fn on_agent_finish(
        &self,
        _agent_id: &str,
        _finish_reason: &str,
        _iterations: u32,
        _duration: Duration,
    ) {
    }

    /// Agent run failed.
    async fn on_agent_error(&self, _agent_id: &str, _code: &str, _message: &str) {}
}

/// Observer implementation that drops every event.
pub struct NoopObserver;

#[async_trait]
impl Observer for NoopObserver {}
