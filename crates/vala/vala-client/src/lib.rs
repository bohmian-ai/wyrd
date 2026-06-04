//! Client-tier Vala observation boundary.
//!
//! This crate owns the typed records that Wyrd-side integrations enqueue for
//! Vala. Transport, batching, retry, and back-pressure belong behind
//! implementations of [`ValaClient`].

#![deny(missing_docs)]
#![allow(clippy::module_name_repetitions)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

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

/// Shared fields present on every Skald-originated Vala observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationEnvelope {
    /// Vala observation identifier.
    pub observation_id: String,
    /// Wyrd run identifier carried across the agent invocation.
    pub run_id: String,
    /// Agent identifier supplied by Skald.
    pub agent_id: String,
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
