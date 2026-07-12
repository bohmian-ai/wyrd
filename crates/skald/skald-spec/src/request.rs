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
        if value.get("max_tokens").is_some()
            && let Ok(request) = serde_json::from_value::<AnthropicMessagesRequest>(value.clone())
        {
            return Ok(Self::AnthropicMessage(request));
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

#[cfg(test)]
mod round_trip {
    use serde::{Serialize, de::DeserializeOwned};
    use serde_json::{json, value::RawValue};

    use crate::common;
    use crate::wire::anthropic_messages::AnthropicStopReason;
    use crate::{MessageNum, ProviderName, ProviderRequest, ProviderResponse};

    fn round_trip<T>(value: &T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_string(value).unwrap();
        let reparsed: T = serde_json::from_str(&json).unwrap();
        assert_eq!(&reparsed, value);
    }

    #[test]
    fn provider_requests_roundtrip() {
        for request in common::provider_request_variants() {
            round_trip(&request);
        }
    }

    #[test]
    fn provider_responses_roundtrip() {
        for response in common::provider_response_variants() {
            round_trip(&response);
        }
    }

    #[test]
    fn messages_roundtrip() {
        for message in common::message_variants() {
            round_trip(&message);
        }
    }

    #[test]
    fn per_provider_wire_roundtrips() {
        round_trip(&common::openai_chat_request());
        round_trip(&common::openai_chat_response());
        round_trip(&common::openai_responses_request());
        round_trip(&common::openai_responses_response());
        round_trip(&common::anthropic_request());
        for reason in [
            AnthropicStopReason::EndTurn,
            AnthropicStopReason::MaxTokens,
            AnthropicStopReason::StopSequence,
            AnthropicStopReason::ToolUse,
            AnthropicStopReason::PauseTurn,
            AnthropicStopReason::Refusal,
        ] {
            round_trip(&common::anthropic_response(reason));
        }
        round_trip(&common::google_request());
        round_trip(&common::google_response(
            crate::wire::google_generate::GoogleFinishReason::Stop,
        ));
        round_trip(&common::vertex_generate_request());
        round_trip(&common::vertex_predict_request());
        round_trip(&common::vertex_predict_response());
    }

    #[test]
    fn raw_v1_request_roundtrip_preserves_provider_and_body() {
        for provider in [
            ProviderName::OpenAi,
            ProviderName::Anthropic,
            ProviderName::Google,
            ProviderName::Vertex,
            ProviderName::Custom("acme".to_string()),
        ] {
            let request = common::raw_request(provider.clone());
            let json = serde_json::to_string(&request).unwrap();
            let reparsed: ProviderRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(reparsed, request);
            assert_eq!(reparsed.provider(), provider);
        }
    }

    #[test]
    fn raw_v1_response_and_message_roundtrip_preserve_body() {
        let response =
            ProviderResponse::RawV1(RawValue::from_string(json!({"x": 1}).to_string()).unwrap());
        round_trip(&response);

        let message =
            MessageNum::RawV1(RawValue::from_string(json!({"y": 2}).to_string()).unwrap());
        round_trip(&message);
    }
}

#[cfg(test)]
mod settings_flatten {
    use serde_json::{Value, json};

    use crate::{
        AnthropicMessagesRequest, AnthropicMessagesSettings, GoogleGenerateContentRequest,
        OpenAiChatRequest, OpenAiResponsesRequest,
    };

    #[test]
    fn openai_settings_flatten_to_top_level_json() {
        let request: OpenAiChatRequest = serde_json::from_value(json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hello"}],
            "temperature": 0.25,
            "seed": 42,
            "logit_bias": {"123": -100}
        }))
        .unwrap();

        assert_eq!(request.settings.seed, Some(42));
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["temperature"], json!(0.25));
        assert_eq!(value["seed"], json!(42));
        assert_eq!(value["logit_bias"], json!({"123": -100}));
        assert!(value.get("settings").is_none());
    }

    #[test]
    fn openai_unmodeled_field_passthrough() {
        let request: OpenAiChatRequest = serde_json::from_value(json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hello"}],
            "future_knob": {"enabled": true}
        }))
        .unwrap();

        assert_eq!(
            request.settings.extra.get("future_knob"),
            Some(&json!({"enabled": true}))
        );
        assert_eq!(
            serde_json::to_value(request).unwrap()["future_knob"],
            json!({"enabled": true})
        );
    }

    #[test]
    fn anthropic_default_max_tokens_is_wire_default() {
        assert_eq!(AnthropicMessagesSettings::default().max_tokens, 4096);

        let request = AnthropicMessagesRequest {
            model: "claude-sonnet-4-5".to_owned(),
            messages: Vec::new(),
            system: None,
            stream: None,
            tools: None,
            tool_choice: None,
            output_config: None,
            settings: AnthropicMessagesSettings::default(),
        };
        assert_eq!(serde_json::to_value(request).unwrap()["max_tokens"], 4096);
    }

    #[test]
    fn google_generation_config_stays_nested() {
        let request: GoogleGenerateContentRequest = serde_json::from_value(json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "generation_config": {
                "temperature": 0.125,
                "thinking_config": {"include_thoughts": true},
                "future_generation_knob": "native"
            }
        }))
        .unwrap();

        let config = request.settings.generation_config.as_ref().unwrap();
        assert_eq!(config.temperature, Some(0.125));
        assert_eq!(
            config.extra.get("future_generation_knob"),
            Some(&Value::String("native".to_owned()))
        );
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["generation_config"]["temperature"], json!(0.125));
        assert!(value.get("temperature").is_none());
    }

    #[test]
    fn google_unknown_top_level_field_is_absorbed() {
        let request: GoogleGenerateContentRequest = serde_json::from_value(json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "native_future": 1
        }))
        .unwrap();

        assert_eq!(request.settings.extra.get("native_future"), Some(&json!(1)));
        assert_eq!(
            serde_json::to_value(request).unwrap()["native_future"],
            json!(1)
        );
    }

    #[test]
    fn settings_requests_round_trip_each_provider() {
        let openai_chat: OpenAiChatRequest = serde_json::from_value(json!({
            "model": "gpt-4o",
            "messages": [{"role": "user", "content": "hello"}],
            "temperature": 0.25,
            "foo": "bar"
        }))
        .unwrap();
        round_trip(openai_chat);

        let openai_responses: OpenAiResponsesRequest = serde_json::from_value(json!({
            "model": "gpt-4o",
            "input": [{"type": "message", "role": "user", "content": [{"type": "input_text", "text": "hello"}]}],
            "max_output_tokens": 64,
            "foo": "bar"
        }))
        .unwrap();
        round_trip(openai_responses);

        let anthropic: AnthropicMessagesRequest = serde_json::from_value(json!({
            "model": "claude-sonnet-4-5",
            "messages": [{"role": "user", "content": [{"type": "text", "text": "hello"}]}],
            "max_tokens": 512,
            "foo": "bar"
        }))
        .unwrap();
        round_trip(anthropic);

        let google: GoogleGenerateContentRequest = serde_json::from_value(json!({
            "contents": [{"role": "user", "parts": [{"text": "hello"}]}],
            "cached_content": "cachedContents/abc",
            "foo": "bar"
        }))
        .unwrap();
        round_trip(google);
    }

    fn round_trip<T>(value: T)
    where
        T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let json = serde_json::to_value(&value).unwrap();
        let reparsed: T = serde_json::from_value(json).unwrap();
        assert_eq!(reparsed, value);
    }
}

#[cfg(test)]
mod untagged_dispatch {
    use serde_json::json;

    use crate::common;
    use crate::{MessageNum, ProviderRequest, ProviderResponse};

    #[test]
    fn provider_request_untagged_dispatch_per_provider() {
        assert!(matches!(
            serde_json::from_value::<ProviderRequest>(
                serde_json::to_value(common::openai_chat_request()).unwrap()
            )
            .unwrap(),
            ProviderRequest::OpenAiChatCompletion(_)
        ));
        assert!(matches!(
            serde_json::from_value::<ProviderRequest>(
                serde_json::to_value(common::anthropic_request()).unwrap()
            )
            .unwrap(),
            ProviderRequest::AnthropicMessage(_)
        ));
        assert!(matches!(
            serde_json::from_value::<ProviderRequest>(
                serde_json::to_value(common::google_request()).unwrap()
            )
            .unwrap(),
            ProviderRequest::GeminiGenerateContent(_)
        ));
    }

    #[test]
    fn vertex_generate_body_is_transparent_google_shape() {
        assert!(matches!(
            serde_json::from_value::<ProviderRequest>(
                serde_json::to_value(common::vertex_generate_request()).unwrap()
            )
            .unwrap(),
            ProviderRequest::GeminiGenerateContent(_)
        ));
    }

    #[test]
    fn raw_v1_is_last_fallback_for_wrapped_unknown_request() {
        let body = json!({"provider": "open_ai", "body": {"untyped": true}});
        assert!(matches!(
            serde_json::from_value::<ProviderRequest>(body).unwrap(),
            ProviderRequest::RawV1 { .. }
        ));
    }

    #[test]
    fn deny_unknown_fields_blocks_cross_variant_confusion() {
        let body = json!({
            "provider": "open_ai",
            "body": {
                "model": "gpt-4o",
                "messages": [{"role": "user", "content": "hi"}],
                "unknown": true
            }
        });
        assert!(matches!(
            serde_json::from_value::<ProviderRequest>(body).unwrap(),
            ProviderRequest::RawV1 { .. }
        ));
    }

    #[test]
    fn message_num_untagged_dispatch_per_provider() {
        assert!(matches!(
            serde_json::from_value::<MessageNum>(
                serde_json::to_value(common::openai_message()).unwrap()
            )
            .unwrap(),
            MessageNum::OpenAi(_)
        ));
        assert!(matches!(
            serde_json::from_value::<MessageNum>(
                serde_json::to_value(common::anthropic_message()).unwrap()
            )
            .unwrap(),
            MessageNum::Anthropic(_)
        ));
        assert!(matches!(
            serde_json::from_value::<MessageNum>(
                serde_json::to_value(common::google_message()).unwrap()
            )
            .unwrap(),
            MessageNum::Gemini(_)
        ));
    }

    #[test]
    fn provider_response_untagged_dispatch_per_provider() {
        assert!(matches!(
            serde_json::from_value::<ProviderResponse>(
                serde_json::to_value(common::openai_chat_response()).unwrap()
            )
            .unwrap(),
            ProviderResponse::OpenAiChatCompletion(_)
        ));
        assert!(matches!(
            serde_json::from_value::<ProviderResponse>(
                serde_json::to_value(common::anthropic_response(
                    crate::wire::anthropic_messages::AnthropicStopReason::EndTurn
                ))
                .unwrap()
            )
            .unwrap(),
            ProviderResponse::AnthropicMessage(_)
        ));
        assert!(matches!(
            serde_json::from_value::<ProviderResponse>(
                serde_json::to_value(common::google_response(
                    crate::wire::google_generate::GoogleFinishReason::Stop
                ))
                .unwrap()
            )
            .unwrap(),
            ProviderResponse::GeminiGenerateContent(_)
        ));
    }
}
