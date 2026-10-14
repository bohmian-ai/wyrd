use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::wire::common::TokenUsage;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// `OpenAI` Responses API request body carried by a prompt or gateway call.
///
/// `input` keeps the form the author or caller sent, either text shorthand or
/// an item list, so the request serializes back without reshaping.
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiResponsesRequest {
    pub model: String,
    /// Conversation input, in the text or item form the author or caller sent.
    pub input: OpenAiResponsesInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<OpenAiResponsesText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiResponsesTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<OpenAiResponsesToolChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(flatten)]
    pub settings: OpenAiResponsesSettings,
}

/// Native OpenAI Responses generation settings flattened into the request body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiResponsesSettings {
    /// Sampling temperature.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling probability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Maximum output-token count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// OpenAI Responses reasoning configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<OpenAiReasoning>,
    /// Whether the provider may store the response.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    /// Additional response fields to include.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    /// Provider metadata object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
    /// Unmodeled OpenAI Responses fields, flattened to the native request location.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiResponsesText {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<OpenAiTextResponseFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiReasoning {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<OpenAiReasoningEffort>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<OpenAiReasoningSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generate_summary: Option<OpenAiReasoningSummary>,
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
pub enum OpenAiReasoningSummary {
    Auto,
    Concise,
    Detailed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OpenAiTextResponseFormat {
    Text,
    JsonObject,
    JsonSchema {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        schema: Option<Map<String, Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum OpenAiResponsesToolChoice {
    Mode(OpenAiResponsesToolChoiceMode),
    Allowed(OpenAiResponsesAllowedToolsChoice),
    Hosted(OpenAiResponsesHostedToolChoice),
    Function(OpenAiResponsesFunctionToolChoice),
    Mcp(OpenAiResponsesMcpToolChoice),
    Custom(OpenAiResponsesCustomToolChoice),
    ApplyPatch(OpenAiResponsesApplyPatchToolChoice),
    Shell(OpenAiResponsesShellToolChoice),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiResponsesToolChoiceMode {
    None,
    Auto,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiResponsesAllowedToolsChoice {
    #[serde(rename = "type")]
    pub kind: OpenAiResponsesAllowedToolsKind,
    pub mode: OpenAiResponsesAllowedToolsMode,
    pub tools: Vec<Map<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiResponsesAllowedToolsKind {
    AllowedTools,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiResponsesAllowedToolsMode {
    Auto,
    Required,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiResponsesHostedToolChoice {
    #[serde(rename = "type")]
    pub kind: OpenAiResponsesHostedToolKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiResponsesHostedToolKind {
    FileSearch,
    WebSearchPreview,
    Computer,
    ComputerUsePreview,
    ComputerUse,
    #[serde(rename = "web_search_preview_2025_03_11")]
    WebSearchPreview20250311,
    ImageGeneration,
    CodeInterpreter,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiResponsesFunctionToolChoice {
    #[serde(rename = "type")]
    pub kind: OpenAiResponsesFunctionToolKind,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiResponsesFunctionToolKind {
    Function,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiResponsesMcpToolChoice {
    #[serde(rename = "type")]
    pub kind: OpenAiResponsesMcpToolKind,
    pub server_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiResponsesMcpToolKind {
    Mcp,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiResponsesCustomToolChoice {
    #[serde(rename = "type")]
    pub kind: OpenAiResponsesCustomToolKind,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiResponsesCustomToolKind {
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiResponsesApplyPatchToolChoice {
    #[serde(rename = "type")]
    pub kind: OpenAiResponsesApplyPatchToolKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiResponsesApplyPatchToolKind {
    ApplyPatch,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiResponsesShellToolChoice {
    #[serde(rename = "type")]
    pub kind: OpenAiResponsesShellToolKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiResponsesShellToolKind {
    #[serde(rename = "shell")]
    Shell,
}

/// OpenAI Responses `input`: the text shorthand for one user message, or an
/// ordered list of input items.
///
/// Both forms decode and re-encode unchanged, so a relayed request keeps the
/// form its caller sent. Editing the conversation first expands the text
/// shorthand into the single user message it stands for.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum OpenAiResponsesInput {
    /// Text shorthand for one user message carrying one `input_text` part.
    Text(String),
    /// Ordered input items. Easy input messages (`{"role", "content"}` with
    /// no `type`, or with text `content`) decode as the message item they
    /// stand for.
    Items(
        #[serde(deserialize_with = "easy_items")]
        #[cfg_attr(feature = "schemars", schemars(schema_with = "input_items_schema"))]
        Vec<OpenAiResponseItem>,
    ),
}

/// JSON Schema of the item list [`easy_items`] accepts.
///
/// The derived schema would publish only canonical [`OpenAiResponseItem`]s,
/// rejecting the easy input messages the decoder accepts. Each item is
/// therefore a canonical item or an `OpenAiEasyInputMessage`: a `role` and
/// text or content-part `content`, with `type` omitted or `message`. The easy
/// message is registered as a named definition so generated declarations name
/// it; decoded and re-encoded items stay canonical.
///
/// # Panics
///
/// Panics only if the literal easy-message schema stops being a valid
/// [`schemars::schema::Schema`], a fixed invariant of this function.
#[cfg(feature = "schemars")]
fn input_items_schema(
    generator: &mut schemars::r#gen::SchemaGenerator,
) -> schemars::schema::Schema {
    /// Definition name of the easy input message: the key it is registered
    /// under and the target of each item's `$ref`, so both stay identical.
    const EASY: &str = "OpenAiEasyInputMessage";
    let item = generator.subschema_for::<OpenAiResponseItem>();
    let parts = generator.subschema_for::<Vec<OpenAiResponseContentPart>>();
    let easy = serde_json::from_value(serde_json::json!({
        "description": "Easy input message: `type` may be omitted and `content` may be text.",
        "type": "object",
        "required": ["content", "role"],
        "properties": {
            "content": {"anyOf": [{"type": "string"}, parts]},
            "role": {"type": "string"},
            "type": {"type": "string", "enum": ["message"]}
        }
    }))
    .expect("the easy input message schema literal is a valid schema");
    generator.definitions_mut().insert(EASY.to_owned(), easy);
    let easy_ref = format!("{}{EASY}", generator.settings().definitions_path);
    serde_json::from_value(serde_json::json!({
        "type": "array",
        "items": {"anyOf": [item, {"$ref": easy_ref}]}
    }))
    .expect("the input item list schema literal is a valid schema")
}

/// Decodes Responses input items, accepting `OpenAI`'s easy input message.
///
/// `OpenAI` accepts a message item with no `type` and with text `content`,
/// and stock clients such as the Agents SDK send that form. Each such item is
/// rewritten to its typed equivalent before the typed decode: `type` becomes
/// `message`, and text `content` becomes one `input_text` part (one
/// `output_text` part for an `assistant` message). Every other item decodes
/// unchanged.
///
/// # Errors
///
/// Returns the deserializer's error when the input is not an array or an item
/// does not decode as an [`OpenAiResponseItem`] after the rewrite.
fn easy_items<'de, D>(deserializer: D) -> Result<Vec<OpenAiResponseItem>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<Value>::deserialize(deserializer)?
        .into_iter()
        .map(|mut item| {
            if let Some(fields) = item.as_object_mut() {
                if !fields.contains_key("type") && fields.contains_key("role") {
                    fields.insert("type".to_owned(), Value::from("message"));
                }
                if fields.get("type").and_then(Value::as_str) == Some("message")
                    && let Some(Value::String(text)) = fields.get("content")
                {
                    let part = if fields.get("role").and_then(Value::as_str) == Some("assistant") {
                        "output_text"
                    } else {
                        "input_text"
                    };
                    let content = serde_json::json!([{ "type": part, "text": text }]);
                    fields.insert("content".to_owned(), content);
                }
            }
            OpenAiResponseItem::deserialize(item).map_err(serde::de::Error::custom)
        })
        .collect()
}

impl OpenAiResponsesInput {
    /// The input as items: the items themselves, or the one user message the
    /// text shorthand stands for.
    #[must_use]
    pub fn items(&self) -> Cow<'_, [OpenAiResponseItem]> {
        match self {
            Self::Text(text) => Cow::Owned(vec![Self::user_message(text.clone())]),
            Self::Items(items) => Cow::Borrowed(items),
        }
    }

    /// Mutable items, expanding the text shorthand into its one user message
    /// first so edits apply to the equivalent item form.
    ///
    /// # Panics
    ///
    /// Never in practice: the text form is replaced by the item form before
    /// the items are borrowed.
    pub fn items_mut(&mut self) -> &mut Vec<OpenAiResponseItem> {
        if let Self::Text(text) = self {
            *self = Self::Items(vec![Self::user_message(std::mem::take(text))]);
        }
        match self {
            Self::Items(items) => items,
            Self::Text(_) => unreachable!("the text shorthand was expanded above"),
        }
    }

    /// The user message equivalent to the text shorthand `text`.
    fn user_message(text: String) -> OpenAiResponseItem {
        OpenAiResponseItem::Message {
            role: "user".to_owned(),
            content: vec![OpenAiResponseContentPart::InputText { text }],
        }
    }
}

impl From<Vec<OpenAiResponseItem>> for OpenAiResponsesInput {
    /// Wraps `items` as the item form.
    fn from(items: Vec<OpenAiResponseItem>) -> Self {
        Self::Items(items)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAiResponseItem {
    Message {
        role: String,
        content: Vec<OpenAiResponseContentPart>,
    },
    FunctionCall {
        call_id: String,
        name: String,
        arguments: String,
    },
    FunctionCallOutput {
        call_id: String,
        /// Tool result, as text or as content parts.
        output: OpenAiFunctionCallOutput,
    },
    Reasoning {
        /// Provider item identity, required to replay the item statelessly.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        /// Reasoning summary parts, replayed as returned.
        #[serde(default)]
        summary: Vec<OpenAiReasoningSummaryPart>,
        /// Opaque reasoning state returned when `reasoning.encrypted_content`
        /// is included, required to replay the item without stored state.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        encrypted_content: Option<String>,
    },
    InputFile {
        file_id: String,
    },
    InputImage {
        image_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
}

/// A Responses tool result: text, or content parts such as `input_text`.
///
/// `OpenAI` accepts both forms, and the Agents SDK sends parts. The form the
/// caller sent is kept so the item re-encodes unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum OpenAiFunctionCallOutput {
    /// Plain text result.
    Text(String),
    /// Content parts.
    Parts(Vec<OpenAiResponseContentPart>),
}

impl OpenAiFunctionCallOutput {
    /// The result's text: the text form itself, or the concatenated text of
    /// every `input_text` and `output_text` part, skipping non-text parts.
    #[must_use]
    pub fn text(&self) -> Cow<'_, str> {
        match self {
            Self::Text(text) => Cow::Borrowed(text),
            Self::Parts(parts) => Cow::Owned(
                parts
                    .iter()
                    .filter_map(|part| match part {
                        OpenAiResponseContentPart::InputText { text }
                        | OpenAiResponseContentPart::OutputText { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect(),
            ),
        }
    }
}

impl From<String> for OpenAiFunctionCallOutput {
    /// Wraps `text` as the text form.
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

/// One part of a Responses reasoning item summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAiReasoningSummaryPart {
    /// Summary text.
    SummaryText {
        /// Summary text content.
        text: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAiResponseContentPart {
    InputText {
        text: String,
    },
    OutputText {
        text: String,
    },
    InputImage {
        image_url: String,
        #[serde(default)]
        detail: Option<String>,
    },
    InputFile {
        file_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAiResponsesTool {
    Function {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        parameters: Option<Map<String, Value>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        strict: Option<bool>,
    },
    WebSearchPreview {},
    FileSearch {
        vector_store_ids: Vec<String>,
    },
    CodeInterpreter {},
    ImageGeneration {},
    Mcp {
        server_label: String,
        server_url: String,
        allowed_tools: Option<Vec<String>>,
    },
    Custom {
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        format: Option<OpenAiResponsesCustomToolFormat>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OpenAiResponsesCustomToolFormat {
    Text,
    Grammar { grammar: OpenAiResponsesGrammar },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiResponsesGrammar {
    pub definition: String,
    pub syntax: OpenAiResponsesGrammarSyntax,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OpenAiResponsesGrammarSyntax {
    Lark,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiResponsesResponse {
    pub id: String,
    pub object: String,
    pub model: String,
    pub status: String,
    pub created_at: u64,
    pub output: Vec<OpenAiResponseItem>,
    #[serde(default)]
    pub usage: Option<OpenAiResponsesUsage>,
    #[serde(default)]
    pub previous_response_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiResponsesUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(default)]
    pub input_tokens_details: Option<OpenAiResponsesInputTokensDetails>,
    #[serde(default)]
    pub output_tokens_details: Option<OpenAiResponsesOutputTokensDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiResponsesInputTokensDetails {
    pub cached_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct OpenAiResponsesOutputTokensDetails {
    pub reasoning_tokens: u64,
}

impl From<OpenAiResponsesUsage> for TokenUsage {
    fn from(u: OpenAiResponsesUsage) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: u
                .input_tokens_details
                .map_or(0, |details| details.cached_tokens),
            reasoning_tokens: u
                .output_tokens_details
                .map_or(0, |details| details.reasoning_tokens),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAiResponsesStreamEvent {
    #[serde(rename = "response.created")]
    ResponseCreated { response: OpenAiResponsesResponse },
    #[serde(rename = "response.in_progress")]
    ResponseInProgress { response: OpenAiResponsesResponse },
    #[serde(rename = "response.output_item.added")]
    OutputItemAdded {
        output_index: u32,
        item: OpenAiResponseItem,
    },
    #[serde(rename = "response.output_text.delta")]
    OutputTextDelta {
        item_id: String,
        output_index: u32,
        delta: String,
    },
    #[serde(rename = "response.output_text.done")]
    OutputTextDone {
        item_id: String,
        output_index: u32,
        text: String,
    },
    #[serde(rename = "response.function_call_arguments.delta")]
    FunctionCallArgumentsDelta { item_id: String, delta: String },
    #[serde(rename = "response.function_call_arguments.done")]
    FunctionCallArgumentsDone { item_id: String, arguments: String },
    #[serde(rename = "response.completed")]
    ResponseCompleted { response: OpenAiResponsesResponse },
    #[serde(rename = "response.failed")]
    ResponseFailed { response: OpenAiResponsesResponse },
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{OpenAiResponseContentPart, OpenAiResponseItem, OpenAiResponsesRequest};

    /// Both `input` forms re-encode exactly as sent, and editing text
    /// shorthand expands it into the user message it stands for first.
    ///
    /// # Panics
    ///
    /// Panics when a form does not round-trip or the expansion differs.
    #[test]
    fn responses_input_keeps_its_form_until_edited() {
        let items = json!([{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]}]);
        for input in [json!("hi"), items.clone()] {
            let body = json!({"model": "m", "input": input, "truncation": "auto"});
            let request: OpenAiResponsesRequest =
                serde_json::from_value(body.clone()).expect("both input forms decode");
            assert_eq!(serde_json::to_value(&request).expect("encodes"), body);
            assert_eq!(
                serde_json::to_value(request.input.items()).expect("encodes"),
                items
            );
        }

        let mut request: OpenAiResponsesRequest =
            serde_json::from_value(json!({"model": "m", "input": "hi"})).expect("decodes");
        request.input.items_mut().push(OpenAiResponseItem::Message {
            role: "assistant".to_owned(),
            content: vec![OpenAiResponseContentPart::OutputText {
                text: "hello".to_owned(),
            }],
        });
        assert_eq!(
            serde_json::to_value(&request.input).expect("encodes"),
            json!([
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hi"}]},
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "hello"}]}
            ])
        );
    }

    /// The published input schema names the easy input message beside the
    /// canonical item, so schema consumers accept what the decoder accepts.
    ///
    /// # Panics
    ///
    /// Panics when the input item schema omits either alternative or the easy
    /// message stops requiring `role` and `content`.
    #[cfg(feature = "schemars")]
    #[test]
    fn input_items_schema_admits_easy_messages() {
        let root = schemars::schema_for!(OpenAiResponsesRequest);
        let schema = serde_json::to_value(&root).expect("schema encodes");
        let input = &schema["definitions"]["OpenAiResponsesInput"]["anyOf"];
        assert_eq!(
            input[1]["items"]["anyOf"],
            json!([
                {"$ref": "#/definitions/OpenAiResponseItem"},
                {"$ref": "#/definitions/OpenAiEasyInputMessage"}
            ])
        );
        let easy = &schema["definitions"]["OpenAiEasyInputMessage"];
        assert_eq!(easy["required"], json!(["content", "role"]));
        assert_eq!(easy["properties"]["type"]["enum"], json!(["message"]));
    }

    /// The Agents SDK's request decodes: easy input messages with no `type`
    /// and text `content` become typed messages, an assistant's text becomes
    /// `output_text`, and replayed function items decode alongside them, including a tool
    /// result sent as content parts.
    ///
    /// # Panics
    ///
    /// Panics when the body does not decode or an item has the wrong shape.
    #[test]
    fn easy_input_messages_decode_as_typed_messages() {
        let request: OpenAiResponsesRequest = serde_json::from_value(json!({
            "model": "openai/gpt-6-luna",
            "instructions": "Call echo.",
            "input": [
                {"role": "user", "content": "go"},
                {"type": "message", "role": "assistant", "content": "ok"},
                {"type": "function_call", "id": "fc_1", "call_id": "c1", "name": "echo", "arguments": "{}", "status": "completed"},
                {"type": "function_call_output", "call_id": "c1", "output": [{"type": "input_text", "text": "hi"}]}
            ],
            "include": [],
            "stream": false,
            "reasoning": {"effort": "medium"}
        }))
        .expect("the Agents SDK body decodes");
        assert_eq!(
            serde_json::to_value(&request.input).expect("encodes"),
            json!([
                {"type": "message", "role": "user", "content": [{"type": "input_text", "text": "go"}]},
                {"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "ok"}]},
                {"type": "function_call", "call_id": "c1", "name": "echo", "arguments": "{}"},
                {"type": "function_call_output", "call_id": "c1", "output": [{"type": "input_text", "text": "hi"}]}
            ])
        );
    }
}
