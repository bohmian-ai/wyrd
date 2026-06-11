//! Client-tier Vala observation boundary.
//!
//! This crate owns the typed records that Wyrd-side integrations enqueue for
//! Vala. Transport, batching, retry, and back-pressure belong behind
//! implementations of [`ValaClient`].
//!
//! Span tagging context: `wyrd.run_id` is a span attribute. In local /
//! serverless eval mode, the scenario id is carried as the
//! `wyrd.eval.scenario_id` span attribute. Cross-service ancestry rides the
//! `Wyrd-Request-Id` HTTP header as a label on every emitted observation. The
//! source of truth is `architecture/v1/00-foundations/tracing.md`.

#![deny(missing_docs)]
#![allow(clippy::module_name_repetitions)]

pub mod eval;
#[cfg(feature = "python")]
pub mod python;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use wyrd_spec::vala::ids::{RecordId, RunId};

/// Result returned by Vala client enqueue operations.
pub type ValaClientResult<T> = Result<T, ValaClientError>;

/// Error returned when a Vala observation could not be accepted for enqueue.
#[derive(Debug, Error)]
pub enum ValaClientError {
    /// The client rejected the record before it reached transport.
    #[error("vala observation enqueue failed: {message}")]
    Enqueue {
        /// Human-readable failure detail.
        message: String,
    },
}

/// Client boundary used by Wyrd integrations to emit Vala observations.
pub trait ValaClient: Send + Sync {
    /// Enqueue an agent-start observation.
    fn observe_agent_start(&self, _record: AgentStartRecord) -> ValaClientResult<()> {
        Ok(())
    }

    /// Enqueue a sampled iteration observation.
    fn observe_iteration(&self, _record: IterationRecord) -> ValaClientResult<()> {
        Ok(())
    }

    /// Enqueue a sampled tool-call observation.
    fn observe_tool_call(&self, _record: ToolCallRecord) -> ValaClientResult<()> {
        Ok(())
    }

    /// Enqueue a tool-result observation.
    fn observe_tool_result(&self, _record: ToolResultRecord) -> ValaClientResult<()> {
        Ok(())
    }

    /// Enqueue an agent-finish observation.
    fn observe_agent_finish(&self, _record: AgentFinishRecord) -> ValaClientResult<()> {
        Ok(())
    }

    /// Enqueue an agent-error observation.
    fn observe_agent_error(&self, _record: AgentErrorRecord) -> ValaClientResult<()> {
        Ok(())
    }
}

/// Shared identity fields present on every Vala observation.
///
/// `record_id` is the per-emission identity; `run_id` is the run identity of
/// the agent invocation that produced it. Card identity is not carried here: it
/// derives from `run_id -> run -> card` via the emit-plane token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationEnvelope {
    /// Vala observation identifier.
    pub record_id: RecordId,
    /// Wyrd run identifier carried across the agent invocation.
    pub run_id: RunId,
}

/// Agent-start observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentStartRecord {
    /// Shared observation identity fields.
    pub envelope: ObservationEnvelope,
    /// Maximum number of iterations configured for the run.
    pub iteration_cap: u32,
}

/// Agent iteration observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IterationRecord {
    /// Shared observation identity fields.
    pub envelope: ObservationEnvelope,
    /// 1-based agent-loop iteration number.
    pub iteration: u32,
}

/// Tool-call observation with redacted argument payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRecord {
    /// Shared observation identity fields.
    pub envelope: ObservationEnvelope,
    /// Tool name supplied by the provider response.
    pub tool: String,
    /// JSON argument payload after Wyrd redaction policy has run.
    pub redacted_args: serde_json::Value,
}

/// Tool-result observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResultRecord {
    /// Shared observation identity fields.
    pub envelope: ObservationEnvelope,
    /// Tool name supplied by the provider response.
    pub tool: String,
    /// Whether the tool call returned successfully.
    pub ok: bool,
}

/// Agent-finish observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentFinishRecord {
    /// Shared observation identity fields.
    pub envelope: ObservationEnvelope,
    /// Reduced finish reason serialized in Wyrd/Vala vocabulary.
    pub finish_reason: String,
    /// Number of iterations completed.
    pub iterations: u32,
}

/// Agent-error observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentErrorRecord {
    /// Shared observation identity fields.
    pub envelope: ObservationEnvelope,
    /// Stable Skald error code.
    pub code: String,
    /// Human-readable error detail. Capped at `max_content_chars` by the Wyrd redaction policy before emission.
    pub detail: String,
    /// Optional Wyrd-side error classification.
    pub wyrd_error_code: Option<String>,
}
