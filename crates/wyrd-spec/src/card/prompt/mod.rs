//! Prompt Card spec.
//!
//! This module is a thin Wyrd Card envelope around [`skald_spec::Prompt`].
//! Provider-specific messages, tools, media, cache directives, response
//! formats, and provider knobs stay native inside `skald-spec`.

mod codec;
mod hash;
mod parameter;
mod promptref;
mod spec;
pub mod validate;

pub use codec::{
    CardLoadFormat, parse_card_bytes, parse_spec_bytes, serialize_card, serialize_spec_bytes,
};
pub use hash::compute as content_hash;
pub use parameter::{
    ParameterName, extract_media_placeholders, extract_text_placeholders, is_valid_parameter_name,
};
pub use promptref::PromptRef;
pub use spec::PromptSpec;
pub use validate::{PromptError, validate};

#[cfg(test)]
pub(crate) mod prompt_support {
    #![allow(dead_code)]

    use std::collections::BTreeMap;

    use crate::api_version::ApiVersion;
    use crate::card::prompt::PromptSpec;
    use crate::envelope::{Card, CardKind, Metadata, Relationships, Spec};
    use crate::ids::CardName;
    use serde_json::Value;
    use skald_spec::wire::anthropic_messages::{
        AnthropicCacheControl, AnthropicSystem, AnthropicSystemBlock, AnthropicToolResultContent,
    };
    use skald_spec::wire::openai_chat::OpenAiMessageContent;
    use skald_spec::wire::openai_responses::{OpenAiResponseContentPart, OpenAiResponseItem};
    use skald_spec::wire::vertex_generate::VertexGenerateContentRequest;
    use skald_spec::{
        AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest,
        AnthropicMessagesSettings, GoogleContent, GoogleGenerateContentRequest,
        GoogleGenerateSettings, GooglePart, OpenAiChatMessage, OpenAiChatRequest,
        OpenAiChatSettings, OpenAiResponsesRequest, OpenAiResponsesSettings, Prompt, ProviderName,
        ProviderRequest, ResponseType,
    };
    use wyrd_semver::VersionBlock;

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
}

#[cfg(test)]
mod prompt_codec_tests {
    use super::prompt_support::{prompt_card, prompt_spec, raw_body_text, raw_request};
    use crate::{CardLoadFormat, parse_card_bytes, serialize_card};
    use skald_spec::ProviderName;

    #[test]
    fn parse_serialize_json_card_roundtrip() {
        let card = prompt_card(prompt_spec(
            super::prompt_support::openai_chat_request("hello"),
            Vec::new(),
        ));
        let bytes = serialize_card(CardLoadFormat::Json, &card).expect("serialize");
        let decoded = parse_card_bytes(CardLoadFormat::Json, &bytes).expect("parse");

        assert_eq!(decoded, card);
    }

    #[test]
    fn parse_serialize_yaml_card_roundtrip() {
        let card = prompt_card(prompt_spec(
            super::prompt_support::openai_chat_request("hello"),
            Vec::new(),
        ));
        let bytes = serialize_card(CardLoadFormat::Yaml, &card).expect("serialize");
        let decoded = parse_card_bytes(CardLoadFormat::Yaml, &bytes).expect("parse");

        assert_eq!(decoded, card);
    }

    #[test]
    fn parse_raw_v1_preserves_body_bytes() {
        let card = prompt_card(prompt_spec(
            raw_request(ProviderName::Custom("acme".to_owned())),
            Vec::new(),
        ));
        let original_body = match &card.spec {
            crate::envelope::Spec::Prompt(spec) => raw_body_text(&spec.prompt.request),
            _ => None,
        };
        let bytes = serialize_card(CardLoadFormat::Json, &card).expect("serialize");
        let decoded = parse_card_bytes(CardLoadFormat::Json, &bytes).expect("parse");
        let decoded_body = match &decoded.spec {
            crate::envelope::Spec::Prompt(spec) => raw_body_text(&spec.prompt.request),
            _ => None,
        };

        assert_eq!(decoded_body, original_body);
    }

    #[test]
    fn from_extension_rejects_txt() {
        let err = CardLoadFormat::from_extension(Some("txt")).expect_err("txt rejected");

        assert_eq!(err.code(), "WYRD_PROMPT_400_LOADER_BAD_EXTENSION");
    }
}

#[cfg(test)]
mod prompt_construct_tests {
    use super::prompt_support::{
        anthropic_request, google_request, openai_chat_request, openai_responses_request,
        prompt_spec, raw_request, vertex_request,
    };
    use skald_spec::ProviderName;

    #[test]
    fn new_provider_variants_build() {
        for request in [
            openai_chat_request("hello"),
            openai_responses_request("hello"),
            anthropic_request("hello"),
            google_request("hello"),
            vertex_request("hello"),
        ] {
            let spec = prompt_spec(request, Vec::new());
            assert!(spec.parameters().is_empty());
        }
    }

    #[test]
    fn new_raw_v1_builds_for_every_provider_name() {
        for provider in [
            ProviderName::OpenAi,
            ProviderName::Anthropic,
            ProviderName::Google,
            ProviderName::Vertex,
            ProviderName::Custom("acme".to_owned()),
        ] {
            let spec = prompt_spec(raw_request(provider), Vec::new());
            assert!(spec.is_fully_bound());
        }
    }

    #[test]
    fn parameters_typed_view_returns_validated_names() {
        let spec = prompt_spec(
            openai_chat_request("hello {{name}} from {{city}}"),
            vec!["name", "city"],
        );
        let parameters: Vec<_> = spec
            .parameters()
            .into_iter()
            .map(|name| name.to_string())
            .collect();

        assert_eq!(parameters, vec!["name", "city"]);
    }
}

#[cfg(test)]
mod prompt_content_hash_tests {
    use super::prompt_support::{
        json_schema_response_type, mutate_first_text, openai_chat_request, prompt, prompt_spec,
    };
    use crate::PromptSpec;

    #[test]
    fn hash_stable_when_version_changes() {
        let mut prompt = prompt(openai_chat_request("hello"), Vec::new());
        let first = PromptSpec::new(prompt.clone())
            .expect("valid")
            .content_hash();
        prompt.version = Some("v3".to_owned());
        let second = PromptSpec::new(prompt).expect("valid").content_hash();

        assert_eq!(first, second);
    }

    #[test]
    fn hash_changes_on_model_request_variables_and_response_type_mutations() {
        let base_prompt = prompt(openai_chat_request("hello {{name}}"), vec!["name"]);
        let base = PromptSpec::new(base_prompt.clone())
            .expect("valid")
            .content_hash();

        let mut changed_model = base_prompt.clone();
        changed_model.model = "other-model".to_owned();
        assert_ne!(
            base,
            PromptSpec::new(changed_model)
                .expect("valid")
                .content_hash()
        );

        let mut changed_request = base_prompt.clone();
        mutate_first_text(&mut changed_request.request, "hello {{name}} again");
        assert_ne!(
            base,
            PromptSpec::new(changed_request)
                .expect("valid")
                .content_hash()
        );

        let mut changed_vars = base_prompt.clone();
        mutate_first_text(&mut changed_vars.request, "hello {{name}} {{city}}");
        changed_vars.variables.push("city".to_owned());
        assert_ne!(
            base,
            PromptSpec::new(changed_vars).expect("valid").content_hash()
        );

        let mut changed_response = base_prompt;
        changed_response.response_type = json_schema_response_type();
        assert_ne!(
            base,
            PromptSpec::new(changed_response)
                .expect("valid")
                .content_hash()
        );
    }

    #[test]
    fn hash_byte_identical_across_runs_and_has_expected_format() {
        let first = prompt_spec(openai_chat_request("hello"), Vec::new()).content_hash();
        let second = prompt_spec(openai_chat_request("hello"), Vec::new()).content_hash();

        assert_eq!(first, second);
        assert!(first.starts_with("sha256:"));
        assert_eq!(first.len(), "sha256:".len() + 64);
        assert!(
            first["sha256:".len()..]
                .chars()
                .all(|ch| ch.is_ascii_hexdigit())
        );
    }
}

#[cfg(test)]
mod prompt_declarative_tests {
    use crate::card::prompt::PromptSpec;
    use crate::envelope::{Card, CardKind, Spec};
    use crate::format;
    use skald_spec::{ProviderRequest, ResponseType};

    fn declarative_yaml() -> &'static str {
        r#"
    apiVersion: wyrd/v1
    kind: Prompt
    metadata:
      name: test-prompt
      version: 0.1.0
    spec:
      provider: openai
      model: gpt-4o
      system: "You are {{persona}}."
      messages:
        - "Summarize {{doc}}."
      model_settings:
        temperature: 0.2
    "#
    }

    fn native_yaml(provider: &str, model: &str) -> String {
        // Build via the builder and serialize, to get the native round-trip fixture
        let prompt = skald_spec::authoring::PromptDraft {
            provider: provider.to_owned(),
            model: model.to_owned(),
            operation: None,
            system: None,
            messages: skald_spec::authoring::DraftMessages::Many(vec!["Hello".to_owned()]),
            model_settings: None,
            version: None,
        }
        .compile()
        .unwrap();

        let spec = PromptSpec::new(prompt).unwrap();
        let card = Card {
            api_version: crate::api_version::ApiVersion::v1(),
            kind: CardKind::Prompt,
            metadata: crate::envelope::Metadata {
                name: crate::ids::CardName::new("test").unwrap(),
                version: Some(wyrd_semver::VersionBlock::parse("0.1.0").unwrap().into()),
                bump: None,
                space: None,
                uid: None,
                labels: Default::default(),
                annotations: Default::default(),
                spec_hash: None,
                artifact_hash: None,
                origin: None,
            },
            spec: Spec::Prompt(spec),
            relationships: Default::default(),
            status: None,
        };
        format::yaml::to_string(&card).unwrap()
    }

    #[test]
    fn declarative_yaml_deserializes_as_card() {
        let card: Card = format::yaml::from_str(declarative_yaml()).unwrap();
        assert_eq!(card.kind, CardKind::Prompt);
        let Spec::Prompt(spec) = &card.spec else {
            panic!("expected Prompt spec");
        };
        assert_eq!(spec.prompt.model, "gpt-4o");
        assert!(matches!(
            spec.prompt.request,
            ProviderRequest::OpenAiChatCompletion(_)
        ));
        assert!(spec.prompt.variables.contains(&"persona".to_owned()));
        assert!(spec.prompt.variables.contains(&"doc".to_owned()));
        assert_eq!(spec.prompt.response_type, ResponseType::Text);
    }

    #[test]
    fn declarative_yaml_has_no_type_field() {
        let card: Card = format::yaml::from_str(declarative_yaml()).unwrap();
        let roundtripped = format::yaml::to_string(&card).unwrap();
        assert!(
            !roundtripped.contains("type: Prompt"),
            "spec.type must not appear in serialized output"
        );
        assert!(roundtripped.contains("kind: Prompt"));
    }

    #[test]
    fn declarative_temperature_setting_applied() {
        let card: Card = format::yaml::from_str(declarative_yaml()).unwrap();
        let Spec::Prompt(spec) = &card.spec else {
            panic!()
        };
        let ProviderRequest::OpenAiChatCompletion(req) = &spec.prompt.request else {
            panic!()
        };
        assert_eq!(req.settings.temperature, Some(0.2));
    }

    #[test]
    fn native_format_round_trips() {
        let yaml_str = native_yaml("openai", "gpt-4o");
        assert!(yaml_str.contains("kind: Prompt"));
        assert!(!yaml_str.contains("type: Prompt"));
        let decoded: Card = format::yaml::from_str(&yaml_str).unwrap();
        let re_encoded = format::yaml::to_string(&decoded).unwrap();
        let redecoded: Card = format::yaml::from_str(&re_encoded).unwrap();
        assert_eq!(decoded, redecoded);
    }

    #[test]
    fn api_version_default_when_omitted() {
        let yaml = r#"
    kind: Prompt
    metadata:
      name: test-prompt
      version: 0.1.0
    spec:
      provider: openai
      model: gpt-4o
      messages:
        - Hello
    "#;
        let card: Card = format::yaml::from_str(yaml).unwrap();
        assert_eq!(card.api_version.as_str(), "wyrd/v1");
    }

    #[test]
    fn malformed_provider_raises_error_not_silent() {
        let yaml = r#"
    kind: Prompt
    metadata:
      name: test-prompt
      version: 0.1.0
    spec:
      provider: badprovider
      model: gpt-4o
      messages:
        - Hello
    "#;
        let result: Result<Card, _> = format::yaml::from_str(yaml);
        assert!(result.is_err(), "unknown provider must produce an error");
    }

    #[test]
    fn anthropic_declarative_compiles() {
        let yaml = r#"
    kind: Prompt
    metadata:
      name: test-anthropic
      version: 0.1.0
    spec:
      provider: anthropic
      model: claude-3-5-sonnet-20241022
      system: You are a helpful assistant.
      messages:
        - What is 2+2?
    "#;
        let card: Card = format::yaml::from_str(yaml).unwrap();
        let Spec::Prompt(spec) = &card.spec else {
            panic!()
        };
        assert!(matches!(
            spec.prompt.request,
            ProviderRequest::AnthropicMessage(_)
        ));
    }
}

#[cfg(test)]
mod prompt_envelope_invariants_tests {
    use super::prompt_support::{json_object_schema, openai_chat_request, prompt};
    use crate::PromptSpec;
    use skald_spec::{ProviderRequest, ResponseType};

    #[test]
    fn invariant_invalid_variable_name_rejected() {
        let err = PromptSpec::new(prompt(
            openai_chat_request("hello {{name}}"),
            vec!["bad-name"],
        ))
        .expect_err("invalid name rejected");

        assert_eq!(err.code(), "WYRD_PROMPT_400_INVALID_VARIABLE_NAME");
    }

    #[test]
    fn invariant_duplicate_variable_rejected() {
        let err = PromptSpec::new(prompt(
            openai_chat_request("hello {{name}}"),
            vec!["name", "name"],
        ))
        .expect_err("duplicate rejected");

        assert_eq!(err.code(), "WYRD_PROMPT_409_DUPLICATE_VARIABLE");
    }

    #[test]
    fn invariant_undeclared_placeholder_rejected() {
        let err = PromptSpec::new(prompt(openai_chat_request("hello {{name}}"), Vec::new()))
            .expect_err("undeclared placeholder rejected");

        assert_eq!(err.code(), "WYRD_PROMPT_422_UNDECLARED_PLACEHOLDER");
    }

    #[test]
    fn invariant_unreferenced_variable_rejected() {
        let err = PromptSpec::new(prompt(openai_chat_request("hello"), vec!["name"]))
            .expect_err("unreferenced variable rejected");

        assert_eq!(err.code(), "WYRD_PROMPT_422_UNREFERENCED_VARIABLE");
    }

    #[test]
    fn invariant_undeclared_media_placeholder_rejected() {
        let err = PromptSpec::new(prompt(openai_chat_request("${media:logo}"), Vec::new()))
            .expect_err("undeclared media placeholder rejected");

        assert_eq!(err.code(), "WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER");
    }

    #[test]
    fn invariant_unreferenced_media_variable_rejected() {
        let mut prompt = prompt(openai_chat_request("hello"), Vec::new());
        prompt.media_variables = vec!["logo".to_owned()];
        let err = PromptSpec::new(prompt).expect_err("unreferenced media variable rejected");

        assert_eq!(err.code(), "WYRD_PROMPT_422_UNREFERENCED_MEDIA_VARIABLE");
    }

    #[test]
    fn invariant_empty_model_rejected() {
        let mut prompt = prompt(openai_chat_request("hello"), Vec::new());
        prompt.model = "   ".to_owned();
        let err = PromptSpec::new(prompt).expect_err("empty model rejected");

        assert_eq!(err.code(), "WYRD_PROMPT_400_EMPTY_MODEL");
    }

    #[test]
    fn invariant_non_object_json_schema_rejected() {
        let mut prompt = prompt(openai_chat_request("hello"), Vec::new());
        prompt.response_type = ResponseType::JsonSchema {
            name: "bad".to_owned(),
            schema: serde_json::json!(["not", "object"]),
        };
        let err = PromptSpec::new(prompt).expect_err("schema rejected");

        assert_eq!(err.code(), "WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA");
    }

    #[test]
    fn invariant_object_json_schema_accepts() {
        let mut prompt = prompt(openai_chat_request("hello"), Vec::new());
        prompt.response_type = ResponseType::JsonSchema {
            name: "ok".to_owned(),
            schema: json_object_schema(),
        };

        assert!(PromptSpec::new(prompt).is_ok());
    }

    #[test]
    fn raw_v1_with_no_placeholders_validates() {
        let raw = ProviderRequest::RawV1 {
            provider: skald_spec::ProviderName::Custom("acme".to_owned()),
            body: serde_json::value::to_raw_value(&serde_json::json!({"x": 1})).expect("raw"),
        };

        assert!(PromptSpec::new(prompt(raw, Vec::new())).is_ok());
    }
}

#[cfg(test)]
mod prompt_placeholders_tests {
    use super::prompt_support::{
        anthropic_with_system, anthropic_with_tool_values, openai_chat_request, prompt_spec,
    };
    use crate::extract_text_placeholders;

    #[test]
    fn extracts_placeholder_in_anthropic_system_text() {
        let spec = prompt_spec(anthropic_with_system("system {{topic}}"), vec!["topic"]);

        assert_eq!(
            extract_text_placeholders(&spec).expect("extracts"),
            vec!["topic"]
        );
    }

    #[test]
    fn extracts_placeholder_in_user_message_text() {
        let spec = prompt_spec(openai_chat_request("hello {{name}}"), vec!["name"]);

        assert_eq!(
            extract_text_placeholders(&spec).expect("extracts"),
            vec!["name"]
        );
    }

    #[test]
    fn extracts_placeholder_in_tool_input_and_result_strings() {
        let spec = prompt_spec(anthropic_with_tool_values(), vec!["city"]);

        assert_eq!(
            extract_text_placeholders(&spec).expect("extracts"),
            vec!["city"]
        );
    }

    #[test]
    fn extracts_first_seen_order_stable_and_deduplicated() {
        let spec = prompt_spec(
            openai_chat_request("{{first}} {{second}} {{first}}"),
            vec!["first", "second"],
        );

        assert_eq!(
            extract_text_placeholders(&spec).expect("extracts"),
            vec!["first", "second"]
        );
        assert_eq!(
            extract_text_placeholders(&spec).expect("extracts again"),
            vec!["first", "second"]
        );
    }
}

#[cfg(test)]
mod prompt_promptref_tests {
    use super::prompt_support::{openai_chat_request, prompt_spec};
    use crate::PromptRef;
    use crate::envelope::CardKind;
    use crate::ids::SpaceName;
    use crate::reference::CardRef;

    #[test]
    fn card_variant_roundtrip() {
        let reference = PromptRef::Card(CardRef {
            kind: CardKind::Prompt,
            name: "support_prompt".parse().expect("valid card name"),
            version: "1.0.0".parse().expect("valid version"),
            space: SpaceName::new("default").expect("static space is valid"),
            uid: None,
        });

        let json = serde_json::to_string(&reference).expect("serialize");
        let decoded: PromptRef = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded, reference);
    }

    #[test]
    fn inline_variant_roundtrip() {
        let reference = PromptRef::Inline(Box::new(prompt_spec(
            openai_chat_request("hello"),
            Vec::new(),
        )));

        let json = serde_json::to_string(&reference).expect("serialize");
        let decoded: PromptRef = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(decoded, reference);
    }

    #[test]
    fn rejects_unknown_kind_tag() {
        let err = serde_json::from_value::<PromptRef>(serde_json::json!({
            "kind": "url",
            "value": "https://example.com"
        }))
        .expect_err("unknown tag rejected");

        assert!(err.to_string().contains("unknown variant"));
    }
}

#[cfg(test)]
mod prompt_schema_drift_tests {
    use crate::{ParameterName, PromptRef, PromptSpec};
    use schemars::schema_for;

    #[test]
    fn prompt_spec_schema_matches_golden() {
        assert_schema_matches::<PromptSpec>("prompt_spec");
    }

    #[test]
    fn prompt_ref_schema_matches_golden() {
        assert_schema_matches::<PromptRef>("prompt_ref");
    }

    #[test]
    fn parameter_name_schema_matches_golden() {
        assert_schema_matches::<ParameterName>("parameter_name");
    }

    fn assert_schema_matches<T: schemars::JsonSchema>(name: &str) {
        let mut schema = schema_for!(T);
        schema.meta_schema = Some("https://json-schema.org/draft/2020-12/schema".to_owned());
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&schema).expect("schema serializes")
        );
        let expected = std::fs::read_to_string(format!(
            "{}/tests/schemas/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("golden schema exists");
        assert_eq!(actual, expected, "schema drift: run mise run codegen:regen");
    }
}

#[cfg(test)]
mod prompt_skald_boundary_tests {
    use super::prompt_support::{
        anthropic_with_system, openai_chat_request, prompt_card, prompt_spec, raw_request,
    };
    use skald_spec::{ProviderName, ProviderRequest};

    #[test]
    fn stored_anthropic_promptcard_round_trips_through_gateway_parse_path() {
        assert_stored_equals_proxied(
            anthropic_with_system("hi {{name}}"),
            vec![("name", "world")],
        );
    }

    #[test]
    fn stored_openai_promptcard_round_trips_through_gateway_parse_path() {
        assert_stored_equals_proxied(openai_chat_request("hi {{name}}"), vec![("name", "world")]);
    }

    #[test]
    fn stored_raw_v1_promptcard_round_trips_byte_identical() {
        let request = raw_request(ProviderName::Custom("acme".to_owned()));
        let spec = prompt_spec(request.clone(), Vec::new());
        let card = prompt_card(spec);
        let bytes = serde_json::to_vec(&card).expect("card serializes");
        let decoded: crate::envelope::Card = serde_json::from_slice(&bytes).expect("card parses");
        let rendered = match decoded.spec {
            crate::envelope::Spec::Prompt(spec) => spec.prompt.render(&[]).expect("renders"),
            _ => unreachable!("prompt card"),
        };
        let rendered_bytes = serde_json::to_vec(&rendered).expect("request serializes");
        let proxied: ProviderRequest =
            serde_json::from_slice(&rendered_bytes).expect("gateway parses");

        assert_eq!(rendered, proxied);
        assert_eq!(
            rendered_bytes,
            serde_json::to_vec(&proxied).expect("reserializes")
        );
        assert_eq!(request, proxied);
    }

    fn assert_stored_equals_proxied(request: ProviderRequest, vars: Vec<(&str, &str)>) {
        let variables = vars.iter().map(|(name, _)| *name).collect();
        let spec = prompt_spec(request, variables);
        let card = prompt_card(spec);
        let bytes = serde_json::to_vec(&card).expect("card serializes");
        let decoded: crate::envelope::Card = serde_json::from_slice(&bytes).expect("card parses");
        let rendered = match decoded.spec {
            crate::envelope::Spec::Prompt(spec) => spec.prompt.render(&vars).expect("renders"),
            _ => unreachable!("prompt card"),
        };
        let rendered_bytes = serde_json::to_vec(&rendered).expect("request serializes");
        let proxied: ProviderRequest =
            serde_json::from_slice(&rendered_bytes).expect("gateway parses");

        assert_eq!(rendered, proxied);
        assert_eq!(
            rendered_bytes,
            serde_json::to_vec(&proxied).expect("reserializes")
        );
    }
}
