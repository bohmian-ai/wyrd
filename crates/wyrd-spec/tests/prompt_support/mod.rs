#![allow(dead_code)]

use std::collections::BTreeMap;

use serde_json::Value;
use skald_spec::wire::anthropic_messages::{
    AnthropicCacheControl, AnthropicSystem, AnthropicSystemBlock, AnthropicToolResultContent,
};
use skald_spec::wire::openai_chat::OpenAiMessageContent;
use skald_spec::wire::openai_responses::{OpenAiResponseContentPart, OpenAiResponseItem};
use skald_spec::wire::vertex_generate::VertexGenerateContentRequest;
use skald_spec::{
    AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest, AnthropicMessagesSettings,
    GoogleContent, GoogleGenerateContentRequest, GoogleGenerateSettings, GooglePart,
    OpenAiChatMessage, OpenAiChatRequest, OpenAiChatSettings, OpenAiResponsesRequest,
    OpenAiResponsesSettings, Prompt, ProviderName, ProviderRequest, ResponseType,
};
use wyrd_semver::VersionBlock;
use wyrd_spec::api_version::ApiVersion;
use wyrd_spec::card::prompt::PromptSpec;
use wyrd_spec::envelope::{Card, CardKind, Metadata, Relationships, Spec};
use wyrd_spec::ids::CardName;

pub fn prompt_spec(request: ProviderRequest, variables: Vec<&str>) -> PromptSpec {
    PromptSpec::new(prompt(request, variables)).expect("static prompt spec is valid")
}

pub fn prompt(request: ProviderRequest, variables: Vec<&str>) -> Prompt {
    Prompt {
        request,
        model: "test-model".to_owned(),
        version: None,
        variables: variables.into_iter().map(ToOwned::to_owned).collect(),
        media_variables: Vec::new(),
        response_type: ResponseType::Text,
    }
}

pub fn prompt_card(spec: PromptSpec) -> Card {
    Card {
        api_version: ApiVersion::v1(),
        kind: CardKind::Prompt,
        metadata: Metadata {
            name: CardName::new("support_prompt").expect("static card name is valid"),
            version: Some(
                VersionBlock::parse("1.0.0")
                    .expect("static version is valid")
                    .into(),
            ),
            bump: None,
            space: None,
            uid: None,
            labels: BTreeMap::new(),
            annotations: BTreeMap::new(),
            spec_hash: None,
            artifact_hash: None,
            origin: None,
        },
        spec: Spec::Prompt(spec),
        relationships: Relationships::default(),
        status: None,
    }
}

pub fn openai_chat_request(text: &str) -> ProviderRequest {
    ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: "gpt-4o".to_owned(),
        messages: vec![OpenAiChatMessage {
            role: "user".to_owned(),
            content: Some(OpenAiMessageContent::Text(text.to_owned())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
            annotations: Vec::new(),
            audio: None,
        }],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: OpenAiChatSettings::default(),
    })
}

pub fn openai_responses_request(text: &str) -> ProviderRequest {
    ProviderRequest::OpenAiResponses(OpenAiResponsesRequest {
        model: "gpt-4o".to_owned(),
        input: vec![OpenAiResponseItem::Message {
            role: "user".to_owned(),
            content: vec![OpenAiResponseContentPart::InputText {
                text: text.to_owned(),
            }],
        }],
        instructions: None,
        text: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        previous_response_id: None,
        stream: None,
        settings: OpenAiResponsesSettings::default(),
    })
}

pub fn anthropic_request(text: &str) -> ProviderRequest {
    ProviderRequest::AnthropicMessage(AnthropicMessagesRequest {
        model: "claude-sonnet-4-5".to_owned(),
        messages: vec![AnthropicMessage {
            role: "user".to_owned(),
            content: vec![AnthropicContentBlock::Text {
                text: text.to_owned(),
                cache_control: None,
                citations: None,
            }],
        }],
        system: None,
        stream: None,
        tools: None,
        tool_choice: None,
        output_config: None,
        settings: AnthropicMessagesSettings {
            max_tokens: 128,
            ..AnthropicMessagesSettings::default()
        },
    })
}

pub fn anthropic_with_system(text: &str) -> ProviderRequest {
    let mut request = match anthropic_request("hello") {
        ProviderRequest::AnthropicMessage(request) => request,
        _ => unreachable!("helper returns Anthropic"),
    };
    request.system = Some(AnthropicSystem::Blocks(vec![AnthropicSystemBlock::Text {
        text: text.to_owned(),
        cache_control: Some(AnthropicCacheControl {
            kind: "ephemeral".to_owned(),
            ttl: Some("1h".to_owned()),
        }),
    }]));
    ProviderRequest::AnthropicMessage(request)
}

pub fn anthropic_with_tool_values() -> ProviderRequest {
    let mut request = match anthropic_request("hello") {
        ProviderRequest::AnthropicMessage(request) => request,
        _ => unreachable!("helper returns Anthropic"),
    };
    request.messages.push(AnthropicMessage {
        role: "assistant".to_owned(),
        content: vec![AnthropicContentBlock::ToolUse {
            id: "toolu_1".to_owned(),
            name: "lookup".to_owned(),
            input: serde_json::json!({ "query": "{{city}}" }),
        }],
    });
    request.messages.push(AnthropicMessage {
        role: "user".to_owned(),
        content: vec![AnthropicContentBlock::ToolResult {
            tool_use_id: "toolu_1".to_owned(),
            content: AnthropicToolResultContent::Text("result for {{city}}".to_owned()),
            is_error: None,
            cache_control: None,
        }],
    });
    ProviderRequest::AnthropicMessage(request)
}

pub fn google_request(text: &str) -> ProviderRequest {
    ProviderRequest::GeminiGenerateContent(GoogleGenerateContentRequest {
        contents: vec![GoogleContent {
            role: "user".to_owned(),
            parts: vec![GooglePart::Text {
                text: text.to_owned(),
            }],
        }],
        system_instruction: None,
        tools: None,
        tool_config: None,
        settings: GoogleGenerateSettings::default(),
    })
}

pub fn vertex_request(text: &str) -> ProviderRequest {
    let google = match google_request(text) {
        ProviderRequest::GeminiGenerateContent(request) => request,
        _ => unreachable!("helper returns Google"),
    };
    ProviderRequest::Vertex(VertexGenerateContentRequest(google))
}

pub fn raw_request(provider: ProviderName) -> ProviderRequest {
    ProviderRequest::RawV1 {
        provider,
        body: serde_json::value::to_raw_value(&serde_json::json!({
            "native": true,
            "nested": { "b": 2, "a": 1 }
        }))
        .expect("raw value builds"),
    }
}

pub fn json_schema_response_type() -> ResponseType {
    ResponseType::JsonSchema {
        name: "answer".to_owned(),
        schema: serde_json::json!({
            "type": "object",
            "properties": { "answer": { "type": "string" } }
        }),
    }
}

pub fn mutate_first_text(request: &mut ProviderRequest, text: &str) {
    match request {
        ProviderRequest::OpenAiChatCompletion(request) => {
            request.messages[0].content = Some(OpenAiMessageContent::Text(text.to_owned()));
        }
        ProviderRequest::AnthropicMessage(request) => {
            request.messages[0].content = vec![AnthropicContentBlock::Text {
                text: text.to_owned(),
                cache_control: None,
                citations: None,
            }];
        }
        ProviderRequest::GeminiGenerateContent(request) => {
            request.contents[0].parts = vec![GooglePart::Text {
                text: text.to_owned(),
            }];
        }
        _ => {}
    }
}

pub fn raw_body_text(request: &ProviderRequest) -> Option<&str> {
    match request {
        ProviderRequest::RawV1 { body, .. } => Some(body.get()),
        _ => None,
    }
}

pub fn json_object_schema() -> Value {
    serde_json::json!({ "type": "object" })
}
