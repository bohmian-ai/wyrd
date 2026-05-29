use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiResponsesRequest {
    pub model: String,
    pub input: Vec<OpenAiResponseItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<OpenAiResponsesText>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<OpenAiResponsesTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Map<String, Value>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct OpenAiResponsesText {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<Value>,
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
        output: String,
    },
    Reasoning {
        #[serde(default)]
        summary: Option<String>,
        #[serde(default)]
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
        description: Option<String>,
        parameters: Value,
        strict: bool,
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
        parameters: Value,
    },
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
    pub output_tokens_details: Option<Value>,
    #[serde(default)]
    pub input_tokens_details: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OpenAiResponsesStreamEvent {
    ResponseCreated {
        response: OpenAiResponsesResponse,
    },
    ResponseInProgress {
        response: OpenAiResponsesResponse,
    },
    OutputItemAdded {
        output_index: u32,
        item: OpenAiResponseItem,
    },
    OutputTextDelta {
        item_id: String,
        output_index: u32,
        delta: String,
    },
    OutputTextDone {
        item_id: String,
        output_index: u32,
        text: String,
    },
    FunctionCallArgumentsDelta {
        item_id: String,
        delta: String,
    },
    FunctionCallArgumentsDone {
        item_id: String,
        arguments: String,
    },
    ResponseCompleted {
        response: OpenAiResponsesResponse,
    },
    ResponseFailed {
        response: OpenAiResponsesResponse,
        error: Value,
    },
}
