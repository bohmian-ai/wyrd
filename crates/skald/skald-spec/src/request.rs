use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

use crate::wire::anthropic_messages::AnthropicMessagesRequest;
use crate::wire::anthropic_messages::AnthropicTool;
use crate::wire::google_embeddings::GoogleBatchEmbedRequest;
use crate::wire::google_generate::{
    GoogleFunctionDeclaration, GoogleGenerateContentRequest, GoogleTool,
};
use crate::wire::openai_chat::OpenAiChatRequest;
use crate::wire::openai_chat::{OpenAiFunction, OpenAiTool};
use crate::wire::openai_embeddings::OpenAiEmbeddingsRequest;
use crate::wire::openai_responses::OpenAiResponsesRequest;
use crate::wire::openai_responses::OpenAiResponsesTool;
use crate::wire::vertex_generate::VertexGenerateContentRequest;
use crate::wire::vertex_predict::VertexPredictRequest;

/// One native LLM request.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(untagged)]
#[non_exhaustive]
pub enum ProviderRequest {
    /// OpenAI Chat Completions request.
    OpenAiChatCompletion(OpenAiChatRequest),
    /// Custom provider that accepts OpenAI Chat Completions request semantics.
    OpenAiChatCompatible {
        /// Provider dispatch target.
        provider: ProviderName,
        /// OpenAI Chat-shaped request body.
        request: OpenAiChatRequest,
    },
    /// OpenAI Responses API request.
    OpenAiResponses(OpenAiResponsesRequest),
    /// OpenAI Embeddings request.
    OpenAiEmbeddings(OpenAiEmbeddingsRequest),
    /// Anthropic Messages request.
    AnthropicMessage(AnthropicMessagesRequest),
    /// Google Gemini GenerateContent request.
    GeminiGenerateContent(GoogleGenerateContentRequest),
    /// Google Gemini BatchEmbedContents request.
    GoogleBatchEmbed(GoogleBatchEmbedRequest),
    /// Vertex GenerateContent request.
    Vertex(VertexGenerateContentRequest),
    /// Vertex Predict request.
    VertexPredict(VertexPredictRequest),
    /// Raw provider request body that no typed variant claimed.
    RawV1 {
        /// Provider dispatch target for the raw body.
        provider: ProviderName,
        /// Unmodified raw provider request JSON.
        #[cfg_attr(feature = "schemars", schemars(with = "serde_json::Value"))]
        body: Box<RawValue>,
    },
}

impl<'de> Deserialize<'de> for ProviderRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;

        if let Ok(raw) = serde_json::from_value::<RawProviderRequest>(value.clone()) {
            return Ok(Self::RawV1 {
                provider: raw.provider,
                body: raw.body,
            });
        }
        if let Ok(request) = serde_json::from_value::<OpenAiChatCompatibleRequest>(value.clone()) {
            return Ok(Self::OpenAiChatCompatible {
                provider: request.provider,
                request: request.request,
            });
        }

        // Keep the S01 untagged ordering explicit while still giving RawV1 a
        // dependable fallback. Deriving `Deserialize` directly would ask
        // `Box<RawValue>` to deserialize from any JSON value and can make
        // fallback behavior hard to reason about as typed variants evolve.
        if value.get("max_tokens").is_some() {
            if let Ok(request) = serde_json::from_value::<AnthropicMessagesRequest>(value.clone()) {
                return Ok(Self::AnthropicMessage(request));
            }
        }
        if let Ok(request) = serde_json::from_value::<OpenAiChatRequest>(value.clone()) {
            return Ok(Self::OpenAiChatCompletion(request));
        }
        if let Ok(request) = serde_json::from_value::<OpenAiResponsesRequest>(value.clone()) {
            return Ok(Self::OpenAiResponses(request));
        }
        if let Ok(request) = serde_json::from_value::<OpenAiEmbeddingsRequest>(value.clone()) {
            return Ok(Self::OpenAiEmbeddings(request));
        }
        if let Ok(request) = serde_json::from_value::<AnthropicMessagesRequest>(value.clone()) {
            return Ok(Self::AnthropicMessage(request));
        }
        if let Ok(request) = serde_json::from_value::<GoogleGenerateContentRequest>(value.clone()) {
            return Ok(Self::GeminiGenerateContent(request));
        }
        if let Ok(request) = serde_json::from_value::<GoogleBatchEmbedRequest>(value.clone()) {
            return Ok(Self::GoogleBatchEmbed(request));
        }
        // Vertex generate is transparent over the Google request shape. This
        // branch is retained for direct deserialization compatibility, but a
        // bare body that also matches Google is claimed by Google first.
        if let Ok(request) = serde_json::from_value::<VertexGenerateContentRequest>(value.clone()) {
            return Ok(Self::Vertex(request));
        }
        if let Ok(request) = serde_json::from_value::<VertexPredictRequest>(value.clone()) {
            return Ok(Self::VertexPredict(request));
        }
        Err(serde::de::Error::custom(
            "data did not match any ProviderRequest variant",
        ))
    }
}

#[derive(Deserialize)]
struct RawProviderRequest {
    provider: ProviderName,
    body: Box<RawValue>,
}

#[derive(Deserialize)]
struct OpenAiChatCompatibleRequest {
    provider: ProviderName,
    request: OpenAiChatRequest,
}

impl PartialEq for ProviderRequest {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::OpenAiChatCompletion(left), Self::OpenAiChatCompletion(right)) => left == right,
            (
                Self::OpenAiChatCompatible {
                    provider: left_provider,
                    request: left_request,
                },
                Self::OpenAiChatCompatible {
                    provider: right_provider,
                    request: right_request,
                },
            ) => left_provider == right_provider && left_request == right_request,
            (Self::OpenAiResponses(left), Self::OpenAiResponses(right)) => left == right,
            (Self::OpenAiEmbeddings(left), Self::OpenAiEmbeddings(right)) => left == right,
            (Self::AnthropicMessage(left), Self::AnthropicMessage(right)) => left == right,
            (Self::GeminiGenerateContent(left), Self::GeminiGenerateContent(right)) => {
                left == right
            }
            (Self::GoogleBatchEmbed(left), Self::GoogleBatchEmbed(right)) => left == right,
            (Self::Vertex(left), Self::Vertex(right)) => left == right,
            (Self::VertexPredict(left), Self::VertexPredict(right)) => left == right,
            (
                Self::RawV1 {
                    provider: left_provider,
                    body: left_body,
                },
                Self::RawV1 {
                    provider: right_provider,
                    body: right_body,
                },
            ) => left_provider == right_provider && left_body.get() == right_body.get(),
            _ => false,
        }
    }
}

/// Which provider a request targets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProviderName {
    /// OpenAI provider.
    OpenAi,
    /// Anthropic provider.
    Anthropic,
    /// Google AI Studio provider.
    Google,
    /// Google Vertex AI provider.
    Vertex,
    /// Provider not modeled by skald-spec.
    Custom(String),
}

/// Provider-tool descriptor projected into native request tool fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct ToolDescriptor {
    /// Provider-visible function name.
    pub name: String,
    /// Provider-visible function description.
    pub description: String,
    /// JSON Schema for function arguments.
    pub parameters: Value,
}

impl ProviderRequest {
    /// Returns the durable provider dispatch target for every request variant.
    pub fn provider(&self) -> ProviderName {
        match self {
            Self::OpenAiChatCompletion(_)
            | Self::OpenAiResponses(_)
            | Self::OpenAiEmbeddings(_) => ProviderName::OpenAi,
            Self::OpenAiChatCompatible { provider, .. } => provider.clone(),
            Self::AnthropicMessage(_) => ProviderName::Anthropic,
            Self::GeminiGenerateContent(_) | Self::GoogleBatchEmbed(_) => ProviderName::Google,
            Self::Vertex(_) | Self::VertexPredict(_) => ProviderName::Vertex,
            Self::RawV1 { provider, .. } => provider.clone(),
        }
    }

    /// Return a copy of the native request with provider-specific tool fields populated.
    pub fn with_tools(mut self, tools: Vec<ToolDescriptor>) -> Self {
        if tools.is_empty() {
            return self;
        }

        match &mut self {
            Self::OpenAiChatCompletion(request) => {
                request.tools = Some(tools.iter().map(openai_chat_tool).collect());
            }
            Self::OpenAiChatCompatible { request, .. } => {
                request.tools = Some(tools.iter().map(openai_chat_tool).collect());
            }
            Self::OpenAiResponses(request) => {
                request.tools = Some(tools.iter().map(openai_responses_tool).collect());
            }
            Self::AnthropicMessage(request) => {
                request.tools = Some(tools.iter().map(anthropic_tool).collect());
            }
            Self::GeminiGenerateContent(request) => {
                request.tools = Some(vec![google_tool(&tools)]);
            }
            Self::Vertex(request) => {
                request.0.tools = Some(vec![google_tool(&tools)]);
            }
            Self::OpenAiEmbeddings(_)
            | Self::GoogleBatchEmbed(_)
            | Self::VertexPredict(_)
            | Self::RawV1 { .. } => {}
        }

        self
    }
}

fn schema_object(value: &Value) -> Option<Map<String, Value>> {
    value.as_object().cloned()
}

fn openai_chat_tool(tool: &ToolDescriptor) -> OpenAiTool {
    OpenAiTool::Function {
        function: OpenAiFunction {
            name: tool.name.clone(),
            description: Some(tool.description.clone()),
            parameters: schema_object(&tool.parameters),
            strict: None,
        },
    }
}

fn openai_responses_tool(tool: &ToolDescriptor) -> OpenAiResponsesTool {
    OpenAiResponsesTool::Function {
        name: tool.name.clone(),
        description: Some(tool.description.clone()),
        parameters: schema_object(&tool.parameters),
        strict: None,
    }
}

fn anthropic_tool(tool: &ToolDescriptor) -> AnthropicTool {
    AnthropicTool {
        name: tool.name.clone(),
        description: Some(tool.description.clone()),
        input_schema: tool.parameters.clone(),
        cache_control: None,
        kind: None,
        display_width_px: None,
        display_height_px: None,
        display_number: None,
    }
}

fn google_tool(tools: &[ToolDescriptor]) -> GoogleTool {
    GoogleTool {
        function_declarations: Some(
            tools
                .iter()
                .map(|tool| GoogleFunctionDeclaration {
                    name: tool.name.clone(),
                    description: Some(tool.description.clone()),
                    parameters: tool.parameters.clone(),
                })
                .collect(),
        ),
        google_search: None,
        google_search_retrieval: None,
        code_execution: None,
        url_context: None,
    }
}
