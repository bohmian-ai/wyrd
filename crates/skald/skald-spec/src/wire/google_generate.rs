use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::wire::common::TokenUsage;

/// `POST /v1beta/models/{model}:generateContent`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GoogleGenerateContentRequest {
    pub contents: Vec<GoogleContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_instruction: Option<GoogleContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<GoogleTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_config: Option<GoogleToolConfig>,
    #[serde(flatten)]
    pub settings: GoogleGenerateSettings,
}

/// Native Google GenerateContent request settings flattened into the request body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GoogleGenerateSettings {
    /// Native Google generationConfig object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GoogleGenerationConfig>,
    /// Native Google safetySettings list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_settings: Option<Vec<GoogleSafetySetting>>,
    /// Cached content resource name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_content: Option<String>,
    /// Provider labels object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Map<String, Value>>,
    /// Unmodeled Google request fields, flattened to the native request location.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// One Google content turn.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleContent {
    pub role: String,
    pub parts: Vec<GooglePart>,
}

/// Content of one answer candidate.
///
/// Distinct from [`GoogleContent`] because an answer is not a request: Gemini
/// returns `{}` here when it truncates a candidate before the model speaks —
/// a `MAX_TOKENS` candidate whose thinking consumed the whole output budget —
/// so both members are optional. A request still requires its role and parts.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleAnswerContent {
    /// Turn role, absent in a truncated candidate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Turn parts, empty in a truncated candidate.
    #[serde(default)]
    pub parts: Vec<GooglePart>,
}

impl From<GoogleAnswerContent> for GoogleContent {
    /// Carries an answer candidate back into a request turn, naming the role
    /// `model` when the answer omitted it.
    fn from(content: GoogleAnswerContent) -> Self {
        Self {
            role: content.role.unwrap_or_else(|| "model".to_owned()),
            parts: content.parts,
        }
    }
}

/// One content part.
///
/// Decoding is untagged and tries variants in declaration order, so `Thought`
/// precedes `Text`: a part carrying both `thought` and `text` is reasoning, not
/// visible text. Structured variants accept the snake_case names Skald emits and
/// the camelCase names Gemini and Vertex return.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum GooglePart {
    /// Model reasoning text.
    Thought {
        /// Reasoning marker the provider sets on thought parts.
        thought: bool,
        /// Reasoning text.
        text: String,
    },
    /// Visible text.
    Text {
        /// Text content.
        text: String,
    },
    /// Inline base64 media.
    InlineData {
        /// Media payload; decodes from `inline_data` or `inlineData`.
        #[serde(rename = "inline_data", alias = "inlineData")]
        inline_data: GoogleInlineData,
    },
    /// Media referenced by URI.
    FileData {
        /// File reference; decodes from `file_data` or `fileData`.
        #[serde(rename = "file_data", alias = "fileData")]
        file_data: GoogleFileData,
    },
    /// Function call the model requested.
    FunctionCall {
        /// Call; decodes from `function_call` or `functionCall`.
        #[serde(rename = "function_call", alias = "functionCall")]
        function_call: GoogleFunctionCall,
    },
    /// Function result returned to the model.
    FunctionResponse {
        /// Result; decodes from `function_response` or `functionResponse`.
        #[serde(rename = "function_response", alias = "functionResponse")]
        function_response: GoogleFunctionResponse,
    },
    /// Code the model generated for execution.
    ExecutableCode {
        /// Code; decodes from `executable_code` or `executableCode`.
        #[serde(rename = "executable_code", alias = "executableCode")]
        executable_code: GoogleExecutableCode,
    },
    /// Result of executing generated code.
    CodeExecutionResult {
        /// Result; decodes from `code_execution_result` or `codeExecutionResult`.
        #[serde(rename = "code_execution_result", alias = "codeExecutionResult")]
        code_execution_result: GoogleCodeExecutionResult,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleInlineData {
    /// Media type; decodes from `mime_type` or `mimeType`.
    #[serde(alias = "mimeType")]
    pub mime_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleFileData {
    /// Media type; decodes from `mime_type` or `mimeType`.
    #[serde(alias = "mimeType")]
    pub mime_type: String,
    /// File URI; decodes from `file_uri` or `fileUri`.
    #[serde(alias = "fileUri")]
    pub file_uri: String,
}

/// Function call the model requested.
///
/// Decoding is deliberately permissive (no `deny_unknown_fields`): Gemini and
/// Vertex add members such as call identifiers and thought signatures, which are
/// ignored rather than failing the whole answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GoogleFunctionCall {
    pub name: String,
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleFunctionResponse {
    pub name: String,
    pub response: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleExecutableCode {
    pub language: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleCodeExecutionResult {
    pub outcome: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GoogleGenerationConfig {
    /// Sampling temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling probability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-k sampling limit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<u32>,
    /// Number of candidate responses.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_count: Option<u32>,
    /// Maximum output-token count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// Stop sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequences: Option<Vec<String>>,
    /// Response MIME type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<String>,
    /// Response schema object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<Value>,
    /// Presence penalty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Frequency penalty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Provider best-effort deterministic seed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// Requested output modalities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_modalities: Option<Vec<String>>,
    /// Gemini thinking configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<GoogleThinkingConfig>,
    /// Speech configuration object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speech_config: Option<Value>,
    /// Unmodeled Google generation-config fields.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_thoughts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleSafetySetting {
    pub category: String,
    pub threshold: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleTool {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_declarations: Option<Vec<GoogleFunctionDeclaration>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_search: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_search_retrieval: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_execution: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_context: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleFunctionDeclaration {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub parameters: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleToolConfig {
    pub function_calling_config: GoogleFunctionCallingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleFunctionCallingConfig {
    pub mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_function_names: Option<Vec<String>>,
}

/// Response envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoogleGenerateContentResponse {
    #[serde(default)]
    pub candidates: Vec<GoogleCandidate>,
    /// Token usage; decodes from `usage_metadata` or `usageMetadata`.
    #[serde(default, alias = "usageMetadata")]
    pub usage_metadata: Option<GoogleUsageMetadata>,
    /// Model version that answered; decodes from `model_version` or `modelVersion`.
    #[serde(default, alias = "modelVersion")]
    pub model_version: Option<String>,
    /// Prompt safety feedback; decodes from `prompt_feedback` or `promptFeedback`.
    #[serde(default, alias = "promptFeedback")]
    pub prompt_feedback: Option<Value>,
    /// Provider-assigned response identifier, echoed so callers can correlate
    /// the answer with provider-side logs.
    #[serde(default, alias = "responseId", skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    /// RFC 3339 time the provider created the response.
    #[serde(default, alias = "createTime", skip_serializing_if = "Option::is_none")]
    pub create_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GoogleCandidate {
    /// Answer content; `{}` when the candidate was truncated before the model
    /// spoke.
    #[serde(default)]
    pub content: GoogleAnswerContent,
    /// Why generation stopped; decodes from `finish_reason` or `finishReason`.
    #[serde(default, alias = "finishReason")]
    pub finish_reason: Option<GoogleFinishReason>,
    #[serde(default)]
    pub index: Option<u32>,
    /// Safety ratings; decodes from `safety_ratings` or `safetyRatings`.
    #[serde(default, alias = "safetyRatings")]
    pub safety_ratings: Vec<GoogleSafetyRating>,
    /// Citation metadata; decodes from `citation_metadata` or `citationMetadata`.
    #[serde(default, alias = "citationMetadata")]
    pub citation_metadata: Option<Value>,
    /// Grounding metadata; decodes from `grounding_metadata` or `groundingMetadata`.
    #[serde(default, alias = "groundingMetadata")]
    pub grounding_metadata: Option<Value>,
    /// Average token log probability; decodes from `avg_logprobs` or `avgLogprobs`.
    #[serde(default, alias = "avgLogprobs")]
    pub avg_logprobs: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GoogleFinishReason {
    Stop,
    MaxTokens,
    Safety,
    Recitation,
    Language,
    Other,
    Blocklist,
    ProhibitedContent,
    Spii,
    MalformedFunctionCall,
    FinishReasonUnspecified,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GoogleUsageMetadata {
    /// Prompt tokens; decodes from `prompt_token_count` or `promptTokenCount`.
    #[serde(default, alias = "promptTokenCount")]
    pub prompt_token_count: u64,
    /// Candidate output tokens; decodes from `candidates_token_count` or `candidatesTokenCount`.
    #[serde(default, alias = "candidatesTokenCount")]
    pub candidates_token_count: u64,
    /// Total tokens; decodes from `total_token_count` or `totalTokenCount`.
    #[serde(default, alias = "totalTokenCount")]
    pub total_token_count: u64,
    /// Cached prompt tokens; decodes from `cached_content_token_count` or `cachedContentTokenCount`.
    #[serde(default, alias = "cachedContentTokenCount")]
    pub cached_content_token_count: u64,
    /// Reasoning tokens; decodes from `thoughts_token_count` or `thoughtsTokenCount`.
    #[serde(default, alias = "thoughtsTokenCount")]
    pub thoughts_token_count: u64,
}

impl From<GoogleUsageMetadata> for TokenUsage {
    fn from(u: GoogleUsageMetadata) -> Self {
        Self {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: u.cached_content_token_count,
            reasoning_tokens: u.thoughts_token_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct GoogleSafetyRating {
    pub category: String,
    pub probability: String,
    #[serde(default)]
    pub blocked: Option<bool>,
}
