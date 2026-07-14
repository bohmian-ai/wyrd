//! OTel GenAI semantic-convention span record.
//!
//! Computed projection of a [`crate::vala::trace::SpanRecord`] whose
//! attributes carry `gen_ai.*` semantic-convention keys. The projector
//! lives in `vala-bifrost`: it scans the source span's attribute bag,
//! lifts every key in [`crate::vala::trace::attributes::GEN_AI_KEYS`] into
//! a typed column on this record, stores remaining `gen_ai.*` keys in
//! [`GenAiSpanRecord::extra`], and reads `gen_ai.evaluation.result` events
//! into [`GenAiSpanRecord::eval_results`].

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::WyrdError;
use crate::vala::ids::{SpanId, TraceId};
use crate::vala::trace::{GenAiEvalResult, Resource, SpanStatus};

/// OTel GenAI semantic-convention span projection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenAiSpanRecord {
    /// 16-byte trace id. Same as `SpanRecord.trace_id`.
    pub trace_id: TraceId,
    /// 8-byte span id. Same as `SpanRecord.span_id`.
    pub span_id: SpanId,
    /// Parent span id. `None` for root spans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<SpanId>,
    /// Span start time. Same as `SpanRecord.start_time`.
    pub start_time: DateTime<Utc>,
    /// Span end time. Same as `SpanRecord.end_time`.
    pub end_time: DateTime<Utc>,
    /// Query convenience field equal to `(end_time - start_time)` in
    /// milliseconds.
    pub duration_ms: u64,
    /// Span status. Same as `SpanRecord.status`.
    pub status: SpanStatus,
    /// Resource of the producer. Same as `SpanRecord.resource`.
    pub resource: Resource,

    /// `gen_ai.provider.name` - provider identifier.
    ///
    /// Required per current OTel GenAI semantic conventions; non-empty values
    /// are enforced by [`GenAiSpanRecord::validate`].
    pub provider_name: String,
    /// `gen_ai.operation.name` - operation such as `chat`,
    /// `generate_content`, `embeddings`, `execute_tool`, `invoke_agent`, or
    /// `retrieval`.
    pub operation_name: String,
    /// `gen_ai.request.model` - model the producer requested.
    pub request_model: String,
    /// `gen_ai.output.type` - output modality such as `text`, `json`,
    /// `image`, or `speech`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_type: Option<String>,
    /// `gen_ai.conversation.id` - stable conversation identifier across
    /// turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,

    /// `gen_ai.response.model` - model the provider responded with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_model: Option<String>,
    /// `gen_ai.response.id` - provider-side response identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    /// `gen_ai.response.finish_reasons` - per-choice finish reasons.
    #[serde(default)]
    pub response_finish_reasons: Vec<String>,
    /// `gen_ai.response.time_to_first_chunk` - streaming time to first chunk,
    /// in seconds per OTel semantic conventions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_time_to_first_chunk_seconds: Option<f64>,

    /// `gen_ai.request.temperature`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_temperature: Option<f64>,
    /// `gen_ai.request.top_p`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_top_p: Option<f64>,
    /// `gen_ai.request.top_k`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_top_k: Option<u32>,
    /// `gen_ai.request.max_tokens`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_max_tokens: Option<u32>,
    /// `gen_ai.request.frequency_penalty`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_frequency_penalty: Option<f64>,
    /// `gen_ai.request.presence_penalty`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_presence_penalty: Option<f64>,
    /// `gen_ai.request.seed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_seed: Option<i64>,
    /// `gen_ai.request.choice.count`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_choice_count: Option<u32>,
    /// `gen_ai.request.stop_sequences`.
    #[serde(default)]
    pub request_stop_sequences: Vec<String>,
    /// `gen_ai.request.stream`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_stream: Option<bool>,
    /// `gen_ai.request.encoding_formats` - embeddings-specific encoding
    /// formats.
    #[serde(default)]
    pub request_encoding_formats: Vec<String>,

    /// `gen_ai.usage.input_tokens`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_input_tokens: Option<u32>,
    /// `gen_ai.usage.output_tokens`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_output_tokens: Option<u32>,
    /// `gen_ai.usage.cache_creation.input_tokens` - cache creation input
    /// tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_cache_creation_input_tokens: Option<u32>,
    /// `gen_ai.usage.cache_read.input_tokens` - cache hit input tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_cache_read_input_tokens: Option<u32>,
    /// `gen_ai.usage.reasoning.output_tokens` - model-internal reasoning
    /// output tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_reasoning_output_tokens: Option<u32>,

    /// `gen_ai.tool.call.id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// `gen_ai.tool.name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// `gen_ai.tool.type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_type: Option<String>,
    /// `gen_ai.tool.description`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_description: Option<String>,

    /// `gen_ai.agent.name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    /// `gen_ai.agent.id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// `gen_ai.agent.description`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_description: Option<String>,
    /// `gen_ai.agent.version`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_version: Option<String>,

    /// `gen_ai.prompt.name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_name: Option<String>,
    /// `gen_ai.workflow.name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_name: Option<String>,
    /// `gen_ai.data_source.id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_source_id: Option<String>,

    /// `gen_ai.embeddings.dimension.count`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embeddings_dimension_count: Option<u32>,

    /// `server.address` - provider endpoint host. This is a general OTel
    /// semantic-convention key, without a `gen_ai.` prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_address: Option<String>,
    /// `server.port` - provider endpoint port. This is a general OTel
    /// semantic-convention key, without a `gen_ai.` prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_port: Option<u16>,
    /// `error.type` - provider-side error classification. This is a general
    /// OTel semantic-convention key, without a `gen_ai.` prefix.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,

    /// `gen_ai.input.messages` - full input message history. This field is
    /// opt-in and redactable at producer or ingest boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_messages: Option<serde_json::Value>,
    /// `gen_ai.output.messages` - generated output messages. This field is
    /// opt-in and redactable at producer or ingest boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_messages: Option<serde_json::Value>,
    /// `gen_ai.system_instructions` - system-prompt content.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_instructions: Option<serde_json::Value>,
    /// `gen_ai.tool.definitions` - tool schemas supplied to the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_definitions: Option<serde_json::Value>,
    /// `gen_ai.tool.call.arguments` - tool-call argument payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_arguments: Option<serde_json::Value>,
    /// `gen_ai.tool.call.result` - tool-call result payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_result: Option<serde_json::Value>,
    /// `gen_ai.retrieval.documents` - retrieval set passed to the model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_documents: Option<serde_json::Value>,
    /// `gen_ai.retrieval.query.text` - retrieval query string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_query_text: Option<String>,
    /// `gen_ai.retrieval.top_k` - retrieval top-k count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieval_top_k: Option<u32>,

    /// `gen_ai.request.reasoning.level` - requested reasoning effort level.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_reasoning_level: Option<String>,
    /// `gen_ai.conversation.compacted` - whether the conversation history was
    /// compacted before this call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_compacted: Option<bool>,
    /// `gen_ai.prompt.version` - version of the prompt used for this call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_version: Option<String>,

    /// `gen_ai.memory.store.id` - memory store identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_store_id: Option<String>,
    /// `gen_ai.memory.record.id` - memory record identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_record_id: Option<String>,
    /// `gen_ai.memory.record.count` - number of memory records touched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_record_count: Option<u32>,
    /// `gen_ai.memory.query.text` - memory query text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_query_text: Option<String>,
    /// `gen_ai.memory.records` - memory records payload. Opt-in and redactable
    /// at producer or ingest boundaries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_records: Option<serde_json::Value>,

    /// `openai.api.type`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_api_type: Option<String>,
    /// `openai.request.service_tier`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_request_service_tier: Option<String>,
    /// `openai.response.service_tier`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_response_service_tier: Option<String>,
    /// `openai.response.system_fingerprint`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_response_system_fingerprint: Option<String>,

    /// `mcp.session.id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_session_id: Option<String>,
    /// `mcp.method.name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_method_name: Option<String>,
    /// `mcp.protocol.version`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_protocol_version: Option<String>,
    /// `mcp.resource.uri`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_resource_uri: Option<String>,

    /// Extracted `gen_ai.evaluation.result` events on the source span.
    #[serde(default)]
    pub eval_results: Vec<GenAiEvalResult>,

    /// Any `gen_ai.*` attribute on the source span that does not have a
    /// dedicated typed column above.
    #[serde(default)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl GenAiSpanRecord {
    /// Validate GenAI-span invariants.
    ///
    /// Enforces non-empty required GenAI attributes, timestamp ordering,
    /// derived duration consistency, finite floating-point request and
    /// response values, resource validation, and child evaluation-result
    /// validation.
    ///
    /// # Errors
    /// Returns [`WyrdError::Validation`] when any invariant fails.
    pub fn validate(&self) -> Result<(), WyrdError> {
        if self.provider_name.is_empty() {
            return Err(WyrdError::Validation {
                message: "gen_ai_span.provider_name must be non-empty".to_string(),
                details: serde_json::Value::Null,
            });
        }
        if self.operation_name.is_empty() {
            return Err(WyrdError::Validation {
                message: "gen_ai_span.operation_name must be non-empty".to_string(),
                details: serde_json::Value::Null,
            });
        }
        if self.request_model.is_empty() {
            return Err(WyrdError::Validation {
                message: "gen_ai_span.request_model must be non-empty".to_string(),
                details: serde_json::Value::Null,
            });
        }
        if self.end_time < self.start_time {
            return Err(WyrdError::Validation {
                message: format!(
                    "gen_ai_span.end_time {:?} < start_time {:?}",
                    self.end_time, self.start_time
                ),
                details: serde_json::Value::Null,
            });
        }

        let derived = super::duration_ms_from_timestamps(self.start_time, self.end_time);
        if self.duration_ms != derived {
            return Err(WyrdError::Validation {
                message: format!(
                    "gen_ai_span.duration_ms {} != end_time - start_time = {}",
                    self.duration_ms, derived
                ),
                details: serde_json::Value::Null,
            });
        }

        const MAX_BLOB_BYTES: usize = 1_048_576;
        for (field, value) in [
            ("input_messages", &self.input_messages),
            ("output_messages", &self.output_messages),
            ("system_instructions", &self.system_instructions),
            ("tool_definitions", &self.tool_definitions),
            ("tool_call_arguments", &self.tool_call_arguments),
            ("tool_call_result", &self.tool_call_result),
            ("retrieval_documents", &self.retrieval_documents),
            ("memory_records", &self.memory_records),
        ] {
            if let Some(v) = value {
                let size = serde_json::to_vec(v).map(|b| b.len()).unwrap_or(0);
                if size > MAX_BLOB_BYTES {
                    return Err(WyrdError::Validation {
                        message: format!(
                            "gen_ai_span.{field} exceeds {MAX_BLOB_BYTES} bytes, got {size}"
                        ),
                        details: serde_json::Value::Null,
                    });
                }
            }
        }
        if self.extra.len() > 128 {
            return Err(WyrdError::Validation {
                message: format!(
                    "gen_ai_span.extra count must be <= 128, got {}",
                    self.extra.len()
                ),
                details: serde_json::Value::Null,
            });
        }

        for (field, value) in [
            ("request_temperature", self.request_temperature),
            ("request_top_p", self.request_top_p),
            ("request_frequency_penalty", self.request_frequency_penalty),
            ("request_presence_penalty", self.request_presence_penalty),
            (
                "response_time_to_first_chunk_seconds",
                self.response_time_to_first_chunk_seconds,
            ),
        ] {
            if let Some(value) = value
                && !value.is_finite()
            {
                return Err(WyrdError::Validation {
                    message: format!("gen_ai_span.{field} must be finite, got {value}"),
                    details: serde_json::Value::Null,
                });
            }
        }

        self.resource.validate()?;
        for (index, eval_result) in self.eval_results.iter().enumerate() {
            eval_result.validate().map_err(|error| match error {
                WyrdError::Validation { message, .. } => WyrdError::Validation {
                    message: format!("eval_results[{index}]: {message}"),
                    details: serde_json::Value::Null,
                },
                other => other,
            })?;
        }
        Ok(())
    }
}
