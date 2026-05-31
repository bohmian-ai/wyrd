//! Skald observer implementation that emits Vala observation records.
//!
//! Observation enqueue calls are fire-and-forget by design. Vala emission must
//! not change the agent or workflow error path, so enqueue failures are ignored
//! after the client boundary has a chance to record them internally.

use std::sync::Arc;

use serde_json::value::RawValue;
use skald_agent::{FinishReason, Observer};
use tracing::debug_span;
use vala_client::{
    AgentErrorRecord, AgentFinishRecord, AgentStartRecord, IterationRecord, ObservationEnvelope,
    ToolCallRecord, ToolResultRecord, ValaClient,
};

use crate::{ObservationId, RedactionPolicy, RunId, RunIdSource, SamplingPolicy, map_skald_code};

/// Wyrd-side adapter for [`skald_agent::Observer`].
pub struct WyrdObserver {
    run_id: RunId,
    vala_client: Arc<dyn ValaClient>,
    sampling: SamplingPolicy,
    redaction: RedactionPolicy,
}

impl WyrdObserver {
    /// Build an adapter for one run.
    #[must_use]
    pub fn new(
        run_id_source: RunIdSource,
        vala_client: Arc<dyn ValaClient>,
        sampling: SamplingPolicy,
        redaction: RedactionPolicy,
    ) -> Self {
        Self {
            run_id: run_id_source.resolve(),
            vala_client,
            sampling,
            redaction,
        }
    }

    /// Borrow the run id assigned to this observer.
    #[must_use]
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    fn envelope(&self, agent_id: &str) -> ObservationEnvelope {
        ObservationEnvelope {
            observation_id: ObservationId::new().to_string(),
            run_id: self.run_id.to_string(),
            agent_id: agent_id.to_owned(),
        }
    }
}

impl Observer for WyrdObserver {
    /// Emit every agent-start event because it creates the run envelope used by
    /// later sampled observations.
    fn on_agent_start(&self, agent_id: &str, iteration_cap: u32) {
        let record = AgentStartRecord {
            envelope: self.envelope(agent_id),
            iteration_cap,
        };
        let _ = self.vala_client.observe_agent_start(record);
    }

    /// Emit sampled iteration events according to [`SamplingPolicy`].
    fn on_iteration(&self, agent_id: &str, iteration: u32) {
        if !self
            .sampling
            .should_sample_iteration(&self.run_id, iteration)
        {
            return;
        }
        let _span = debug_span!(
            "wyrd_observe.iteration",
            run_id = %self.run_id,
            agent_id,
            iteration
        )
        .entered();
        let record = IterationRecord {
            envelope: self.envelope(agent_id),
            iteration,
        };
        let _ = self.vala_client.observe_iteration(record);
    }

    /// Emit sampled tool-call events after applying [`RedactionPolicy`].
    fn on_tool_call(&self, agent_id: &str, tool: &str, args: &RawValue) {
        if !self.sampling.should_sample_tool_call(&self.run_id, tool) {
            return;
        }
        let redacted = self.redaction.redact_tool_args(tool, args);
        let redacted_args = serde_json::from_str(redacted.get())
            .unwrap_or_else(|_| serde_json::Value::String(crate::REDACTED_PLACEHOLDER.to_owned()));
        let record = ToolCallRecord {
            envelope: self.envelope(agent_id),
            tool: tool.to_owned(),
            redacted_args,
        };
        let _ = self.vala_client.observe_tool_call(record);
    }

    /// Emit every tool-result event so sampled calls have a terminal outcome.
    fn on_tool_result(&self, agent_id: &str, tool: &str, ok: bool) {
        let record = ToolResultRecord {
            envelope: self.envelope(agent_id),
            tool: tool.to_owned(),
            ok,
        };
        let _ = self.vala_client.observe_tool_result(record);
    }

    /// Emit every agent-finish event because it closes the run envelope.
    fn on_agent_finish(&self, agent_id: &str, finish: FinishReason, iterations: u32) {
        let record = AgentFinishRecord {
            envelope: self.envelope(agent_id),
            finish_reason: finish_reason_name(&finish).to_owned(),
            iterations,
        };
        let _ = self.vala_client.observe_agent_finish(record);
    }

    /// Emit every agent-error event so failures remain observable even when
    /// high-volume iteration and tool-call events are sampled.
    fn on_agent_error(&self, agent_id: &str, code: &'static str, detail: &str) {
        let record = AgentErrorRecord {
            envelope: self.envelope(agent_id),
            code: code.to_owned(),
            detail: self.redaction.redact_response_text(detail),
            wyrd_error_code: map_skald_code(code).map(|variant| variant.code()),
        };
        let _ = self.vala_client.observe_agent_error(record);
    }
}

fn finish_reason_name(finish: &FinishReason) -> &'static str {
    match finish {
        FinishReason::Stop => "stop",
        FinishReason::Length => "length",
        FinishReason::ContentFilter => "content_filter",
        FinishReason::ToolCalls => "tool_calls",
        FinishReason::Other => "other",
    }
}
