use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::wire::common::TokenUsage;

/// `POST /v1/chat/completions`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiChatRequest {
    pub model: String,
    pub messages: Vec<OpenAiChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<OpenAiResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_options: Option<OpenAiStreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<OpenAiChatToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(flatten)]
    pub settings: OpenAiChatSettings,
}

/// Native OpenAI Chat generation settings flattened into the request body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiChatSettings {
    /// Sampling temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling probability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Legacy maximum generated-token count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Maximum completion-token count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_completion_tokens: Option<u32>,
    /// Number of completions to generate.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// Stop sequence or sequences.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<OpenAiStop>,
    /// Presence penalty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Frequency penalty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Provider best-effort deterministic seed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    /// Token-bias map keyed by token id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logit_bias: Option<Map<String, Value>>,
    /// Provider-visible end-user identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Requested reasoning effort.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<OpenAiReasoningEffort>,
    /// Requested output modalities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modalities: Option<Vec<OpenAiResponseModality>>,
    /// Audio output settings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<OpenAiChatAudio>,
    /// Prediction content hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prediction: Option<OpenAiPredictionContent>,
    /// OpenAI prompt cache key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    /// OpenAI service tier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    /// Safety identifier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_identifier: Option<String>,
    /// Whether the provider may store the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    /// Provider metadata object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
    /// Whether to return token log probabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<bool>,
    /// Number of top token log probabilities.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_logprobs: Option<u32>,
    /// Unmodeled OpenAI Chat fields, flattened to the native request location.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum OpenAiStop {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiResponseModality {
    Text,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiChatAudio {
    pub voice: OpenAiVoice,
    pub format: OpenAiAudioFormat,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum OpenAiVoice {
    BuiltIn(OpenAiBuiltInVoice),
    Custom(OpenAiCustomVoice),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiBuiltInVoice {
    Alloy,
    Ash,
    Ballad,
    Coral,
    Echo,
    Fable,
    Nova,
    Onyx,
    Sage,
    Shimmer,
    Marin,
    Cedar,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiCustomVoice {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiAudioFormat {
    Wav,
    Aac,
    Mp3,
    Flac,
    Opus,
    Pcm16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiPredictionContent {
    #[serde(rename = "type")]
    pub kind: OpenAiPredictionKind,
    pub content: OpenAiPredictionPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiPredictionKind {
    Content,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum OpenAiPredictionPayload {
    Text(String),
    Parts(Vec<OpenAiPredictionContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OpenAiPredictionContentPart {
    Text { text: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiStreamOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_usage: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_obfuscation: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OpenAiResponseFormat {
    Text,
    JsonObject,
    JsonSchema { json_schema: OpenAiJsonSchema },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiJsonSchema {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OpenAiTool {
    Function { function: OpenAiFunction },
    Custom { custom: OpenAiCustomTool },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiFunction {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiCustomTool {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OpenAiCustomToolFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OpenAiCustomToolFormat {
    Text,
    Grammar { grammar: OpenAiGrammar },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiGrammar {
    pub definition: String,
    pub syntax: OpenAiGrammarSyntax,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiGrammarSyntax {
    Lark,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum OpenAiChatToolChoice {
    Mode(OpenAiToolChoiceMode),
    Allowed(OpenAiAllowedToolsChoice),
    Function(OpenAiNamedFunctionToolChoice),
    Custom(OpenAiNamedCustomToolChoice),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiToolChoiceMode {
    None,
    Auto,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiAllowedToolsChoice {
    #[serde(rename = "type")]
    pub kind: OpenAiAllowedToolsKind,
    pub allowed_tools: OpenAiAllowedTools,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiAllowedToolsKind {
    AllowedTools,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiAllowedTools {
    pub mode: OpenAiAllowedToolsMode,
    pub tools: Vec<Map<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiAllowedToolsMode {
    Auto,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiNamedFunctionToolChoice {
    #[serde(rename = "type")]
    pub kind: OpenAiNamedFunctionToolChoiceKind,
    pub function: OpenAiFunctionChoice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiNamedFunctionToolChoiceKind {
    Function,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiFunctionChoice {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiNamedCustomToolChoice {
    #[serde(rename = "type")]
    pub kind: OpenAiNamedCustomToolChoiceKind,
    pub custom: OpenAiCustomChoice,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiNamedCustomToolChoiceKind {
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiCustomChoice {
    pub name: String,
}

/// One chat message on the wire.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiChatMessage {
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<OpenAiMessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
    /// URL citations from web search, when present.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<OpenAiMessageAnnotation>,
    /// Audio output, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<OpenAiMessageAudio>,
}

/// A URL citation annotation returned by web search.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiMessageAnnotation {
    #[serde(rename = "type")]
    pub kind: String,
    pub url_citation: OpenAiUrlCitation,
}

/// Citation metadata for a web search result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiUrlCitation {
    pub url: String,
    pub title: String,
    pub start_index: u32,
    pub end_index: u32,
}

/// Audio output returned by the model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiMessageAudio {
    pub id: String,
    pub expires_at: u64,
    pub data: String,
    pub transcript: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum OpenAiMessageContent {
    Text(String),
    Parts(Vec<OpenAiContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OpenAiContentPart {
    Text { text: String },
    ImageUrl { image_url: OpenAiImageUrl },
    InputAudio { input_audio: OpenAiInputAudio },
    File { file: OpenAiFilePart },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiInputAudio {
    pub data: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiFilePart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: OpenAiToolFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiToolFunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Response envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiChatResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiChatChoice>,
    #[serde(default)]
    pub usage: Option<OpenAiUsage>,
    #[serde(default)]
    pub system_fingerprint: Option<String>,
    #[serde(default)]
    pub service_tier: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiChatChoice {
    pub index: u32,
    pub message: OpenAiChatMessage,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub logprobs: Option<OpenAiChatLogprobs>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiChatLogprobs {
    #[serde(default)]
    pub content: Vec<Value>,
    #[serde(default)]
    pub refusal: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub prompt_tokens_details: Option<OpenAiPromptTokensDetails>,
    #[serde(default)]
    pub completion_tokens_details: Option<OpenAiCompletionTokensDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiPromptTokensDetails {
    #[serde(default)]
    pub audio_tokens: u64,
    #[serde(default)]
    pub cached_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiCompletionTokensDetails {
    #[serde(default)]
    pub accepted_prediction_tokens: u64,
    #[serde(default)]
    pub audio_tokens: u64,
    #[serde(default)]
    pub reasoning_tokens: u64,
    #[serde(default)]
    pub rejected_prediction_tokens: u64,
}

impl From<OpenAiUsage> for TokenUsage {
    fn from(u: OpenAiUsage) -> Self {
        let cache_read = u
            .prompt_tokens_details
            .as_ref()
            .map(|details| details.cached_tokens)
            .unwrap_or(0);
        let reasoning = u
            .completion_tokens_details
            .as_ref()
            .map(|details| details.reasoning_tokens)
            .unwrap_or(0);
        Self {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: cache_read,
            reasoning_tokens: reasoning,
        }
    }
}

/// Server-sent event chunk shape for streaming.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiChatStreamChunk {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub model: String,
    pub choices: Vec<OpenAiChatStreamChoice>,
    #[serde(default)]
    pub usage: Option<OpenAiUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiChatStreamChoice {
    pub index: u32,
    pub delta: OpenAiChatChoiceDelta,
    #[serde(default)]
    pub finish_reason: Option<String>,
    #[serde(default)]
    pub logprobs: Option<OpenAiChatLogprobs>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiChatChoiceDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
}

#[cfg(test)]
mod openai_wire {
    use serde_json::json;

    use crate::TokenUsage;
    use crate::wire::openai_chat::{
        OpenAiAudioFormat, OpenAiChatRequest, OpenAiChatToolChoice, OpenAiPredictionPayload,
        OpenAiResponseFormat, OpenAiTool, OpenAiUsage, OpenAiVoice,
    };
    use crate::wire::openai_responses::{
        OpenAiResponsesRequest, OpenAiResponsesResponse, OpenAiResponsesStreamEvent,
        OpenAiResponsesToolChoice, OpenAiTextResponseFormat,
    };

    #[test]
    fn chat_request_uses_typed_openai_fields() {
        let body = json!({
            "model": "gpt-4o-audio-preview",
            "messages": [
                {"role": "user", "content": "hello"}
            ],
            "audio": {"voice": "alloy", "format": "mp3"},
            "prediction": {
                "type": "content",
                "content": [{"type": "text", "text": "known output"}]
            },
            "tool_choice": {"type": "function", "function": {"name": "answer"}},
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "answer",
                        "description": "answer the prompt",
                        "parameters": {"type": "object"},
                        "strict": true
                    }
                },
                {
                    "type": "custom",
                    "custom": {
                        "name": "freeform",
                        "format": {
                            "type": "grammar",
                            "grammar": {"definition": "start: WORD", "syntax": "lark"}
                        }
                    }
                }
            ],
            "metadata": {"purpose": "review"},
            "modalities": ["text", "audio"],
            "stream_options": {"include_usage": true, "include_obfuscation": false},
            "logit_bias": {"42": -1},
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": "answer",
                    "schema": {"type": "object"},
                    "strict": true
                }
            }
        });

        let request: OpenAiChatRequest = serde_json::from_value(body.clone()).unwrap();

        assert!(matches!(
            request.settings.audio.as_ref().unwrap().voice,
            OpenAiVoice::BuiltIn(_)
        ));
        assert_eq!(
            request.settings.audio.as_ref().unwrap().format,
            OpenAiAudioFormat::Mp3
        );
        assert!(matches!(
            request.settings.prediction.as_ref().unwrap().content,
            OpenAiPredictionPayload::Parts(_)
        ));
        assert!(matches!(
            request.tool_choice.as_ref().unwrap(),
            OpenAiChatToolChoice::Function(_)
        ));
        assert!(matches!(
            request.tools.as_ref().unwrap().first().unwrap(),
            OpenAiTool::Function { .. }
        ));
        assert_eq!(
            request
                .settings
                .metadata
                .as_ref()
                .unwrap()
                .get("purpose")
                .unwrap(),
            "review"
        );
        assert!(matches!(
            request.response_format.as_ref().unwrap(),
            OpenAiResponseFormat::JsonSchema { .. }
        ));

        let round_trip = serde_json::to_value(request).unwrap();
        assert_eq!(round_trip, body);
    }

    #[test]
    fn responses_request_uses_typed_openai_fields() {
        let body = json!({
            "model": "gpt-5.4",
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "hello"}]
                }
            ],
            "reasoning": {"effort": "high", "summary": "concise"},
            "text": {
                "format": {
                    "type": "json_schema",
                    "name": "answer",
                    "schema": {"type": "object"},
                    "strict": true
                }
            },
            "tool_choice": {"type": "mcp", "server_label": "deepwiki", "name": "lookup"},
            "tools": [
                {
                    "type": "function",
                    "name": "answer",
                    "parameters": {"type": "object"},
                    "strict": true
                },
                {
                    "type": "custom",
                    "name": "freeform",
                    "format": {
                        "type": "grammar",
                        "grammar": {"definition": "start: WORD", "syntax": "regex"}
                    }
                }
            ],
            "metadata": {"purpose": "review"}
        });

        let request: OpenAiResponsesRequest = serde_json::from_value(body.clone()).unwrap();

        assert!(request.settings.reasoning.is_some());
        assert!(matches!(
            request.text.as_ref().unwrap().format.as_ref().unwrap(),
            OpenAiTextResponseFormat::JsonSchema { .. }
        ));
        assert!(matches!(
            request.tool_choice.as_ref().unwrap(),
            OpenAiResponsesToolChoice::Mcp(_)
        ));
        assert_eq!(
            request
                .settings
                .metadata
                .as_ref()
                .unwrap()
                .get("purpose")
                .unwrap(),
            "review"
        );

        let round_trip = serde_json::to_value(request).unwrap();
        assert_eq!(round_trip, body);
    }

    #[test]
    fn nested_openai_chat_contracts_reject_unknown_fields() {
        let invalid_audio: Result<OpenAiChatRequest, _> = serde_json::from_value(json!({
            "model": "gpt-4o-audio-preview",
            "messages": [{"role": "user", "content": "hello"}],
            "audio": {"voice": "alloy", "format": "mp3", "extra": true}
        }));

        assert!(invalid_audio.is_err());
    }

    #[test]
    fn openai_usage_details_are_typed() {
        let usage: OpenAiUsage = serde_json::from_value(json!({
            "prompt_tokens": 10,
            "completion_tokens": 5,
            "total_tokens": 15,
            "prompt_tokens_details": {"audio_tokens": 1, "cached_tokens": 4},
            "completion_tokens_details": {
                "accepted_prediction_tokens": 2,
                "audio_tokens": 0,
                "reasoning_tokens": 3,
                "rejected_prediction_tokens": 1
            }
        }))
        .unwrap();

        let normalized = TokenUsage::from(usage);
        assert_eq!(normalized.cache_read_input_tokens, 4);
        assert_eq!(normalized.reasoning_tokens, 3);
    }

    #[test]
    fn responses_response_accepts_openapi_usage_details() {
        let response: OpenAiResponsesResponse = serde_json::from_value(json!({
            "id": "resp_123",
            "object": "response",
            "created_at": 1741476777,
            "status": "completed",
            "model": "gpt-4o-2024-08-06",
            "output": [
                {
                    "type": "message",
                    "id": "msg_123",
                    "status": "completed",
                    "role": "assistant",
                    "content": [
                        {"type": "output_text", "text": "hello", "annotations": []}
                    ]
                }
            ],
            "usage": {
                "input_tokens": 8,
                "input_tokens_details": {"cached_tokens": 2},
                "output_tokens": 6,
                "output_tokens_details": {"reasoning_tokens": 4},
                "total_tokens": 14
            },
            "previous_response_id": null
        }))
        .unwrap();

        let usage = response.usage.unwrap();
        assert_eq!(usage.input_tokens_details.unwrap().cached_tokens, 2);
        assert_eq!(usage.output_tokens_details.unwrap().reasoning_tokens, 4);
    }

    #[test]
    fn responses_stream_events_use_openapi_discriminators() {
        let event: OpenAiResponsesStreamEvent = serde_json::from_value(json!({
            "type": "response.failed",
            "sequence_number": 3,
            "response": {
                "id": "resp_123",
                "object": "response",
                "created_at": 1740855869,
                "status": "failed",
                "completed_at": null,
                "error": {
                    "code": "server_error",
                    "message": "The model failed to generate a response."
                },
                "model": "gpt-4o-2024-08-06",
                "output": [],
                "usage": {
                    "input_tokens": 0,
                    "input_tokens_details": {"cached_tokens": 0},
                    "output_tokens": 0,
                    "output_tokens_details": {"reasoning_tokens": 0},
                    "total_tokens": 0
                }
            }
        }))
        .unwrap();

        assert!(matches!(
            event,
            OpenAiResponsesStreamEvent::ResponseFailed { .. }
        ));
        assert_eq!(
            serde_json::to_value(OpenAiResponsesStreamEvent::ResponseCompleted {
                response: OpenAiResponsesResponse {
                    id: "resp_123".to_string(),
                    object: "response".to_string(),
                    model: "gpt-4o-2024-08-06".to_string(),
                    status: "completed".to_string(),
                    created_at: 1740855869,
                    output: Vec::new(),
                    usage: None,
                    previous_response_id: None,
                },
            })
            .unwrap()["type"],
            "response.completed"
        );
    }
}
