//! Observer that fans out events to multiple downstream observers.

use std::sync::Arc;
use std::time::Duration;

use crate::Observer;
use async_trait::async_trait;

/// Fans out every observer event to each registered downstream observer
/// in order. Events are dispatched sequentially - a slow observer blocks
/// later ones. Observers are best-effort; failures in one do not affect others.
///
/// Construct via `CompositeObserver::new(vec![obs_a, obs_b])`.
/// The inner list is immutable after construction - no shared mutable state.
pub struct CompositeObserver(Vec<Arc<dyn Observer>>);

impl CompositeObserver {
    /// Create a composite from a list of observers.
    #[must_use]
    pub fn new(observers: Vec<Arc<dyn Observer>>) -> Self {
        Self(observers)
    }
}

#[async_trait]
impl Observer for CompositeObserver {
    async fn on_agent_start(
        &self,
        run_id: &str,
        parent_run_id: Option<&str>,
        agent_id: &str,
        input: &str,
        session_id: Option<&str>,
    ) {
        for obs in &self.0 {
            obs.on_agent_start(run_id, parent_run_id, agent_id, input, session_id)
                .await;
        }
    }

    async fn on_iteration(&self, run_id: &str, agent_id: &str, index: u32) {
        for obs in &self.0 {
            obs.on_iteration(run_id, agent_id, index).await;
        }
    }

    async fn on_model_call(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        provider: &str,
        model: &str,
        request: &skald_spec::ProviderRequest,
    ) {
        for obs in &self.0 {
            obs.on_model_call(run_id, agent_id, iteration, provider, model, request)
                .await;
        }
    }

    async fn on_model_result(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        finish_reason: &str,
        synthetic: bool,
        response: &skald_spec::ProviderResponse,
    ) {
        for obs in &self.0 {
            obs.on_model_result(
                run_id,
                agent_id,
                iteration,
                finish_reason,
                synthetic,
                response,
            )
            .await;
        }
    }

    async fn on_tool_call(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        call_id: &str,
        tool_name: &str,
    ) {
        for obs in &self.0 {
            obs.on_tool_call(run_id, agent_id, iteration, call_id, tool_name)
                .await;
        }
    }

    async fn on_tool_result(
        &self,
        run_id: &str,
        agent_id: &str,
        iteration: u32,
        call_id: &str,
        ok: bool,
    ) {
        for obs in &self.0 {
            obs.on_tool_result(run_id, agent_id, iteration, call_id, ok)
                .await;
        }
    }

    async fn on_agent_finish(
        &self,
        run_id: &str,
        agent_id: &str,
        finish_reason: &str,
        iterations: u32,
        duration: Duration,
    ) {
        for obs in &self.0 {
            obs.on_agent_finish(run_id, agent_id, finish_reason, iterations, duration)
                .await;
        }
    }

    async fn on_agent_error(&self, run_id: &str, agent_id: &str, code: &str, message: &str) {
        for obs in &self.0 {
            obs.on_agent_error(run_id, agent_id, code, message).await;
        }
    }

    async fn on_workflow_start(&self, run_id: &str, workflow_id: &str, step_count: usize) {
        for obs in &self.0 {
            obs.on_workflow_start(run_id, workflow_id, step_count).await;
        }
    }

    async fn on_workflow_finish(&self, run_id: &str, workflow_id: &str, duration: Duration) {
        for obs in &self.0 {
            obs.on_workflow_finish(run_id, workflow_id, duration).await;
        }
    }
}
