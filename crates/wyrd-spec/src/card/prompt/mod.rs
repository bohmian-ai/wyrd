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
    use std::collections::BTreeMap;

    use crate::api_version::ApiVersion;
    use crate::card::prompt::PromptSpec;
    use crate::envelope::{Card, CardKind, Metadata, Relationships, Spec};
    use crate::ids::CardName;
    use skald_spec::wire::anthropic_messages::{
        AnthropicCacheControl, AnthropicSystem, AnthropicSystemBlock, AnthropicToolResultContent,
    };
    use skald_spec::wire::openai_chat::{
        OpenAiJsonSchema, OpenAiMessageContent, OpenAiResponseFormat,
    };
    use skald_spec::wire::openai_responses::{OpenAiResponseContentPart, OpenAiResponseItem};
    use skald_spec::{
        AnthropicContentBlock, AnthropicMessage, AnthropicMessagesRequest,
        AnthropicMessagesSettings, GoogleContent, GoogleGenerateContentRequest,
        GoogleGenerateSettings, GooglePart, OpenAiChatMessage, OpenAiChatRequest,
        OpenAiChatSettings, OpenAiResponsesRequest, OpenAiResponsesSettings, Prompt, ProviderName,
        ProviderRequest,
    };
    use wyrd_semver::VersionBlock;

    pub fn prompt_spec(request: ProviderRequest, variables: Vec<&str>) -> PromptSpec {
        PromptSpec::new(prompt(request, variables)).expect("static prompt spec is valid")
    }

    /// Builds a Prompt from `request`, whose literal model is the body's own
    /// model (`test-model` for a model-less GenerateContent body), and
    /// asserts it derives `variables`.
    ///
    /// # Panics
    /// Panics when `request` cannot be serialized, is not a valid Prompt, or
    /// derives other variables.
    pub fn prompt(request: ProviderRequest, variables: Vec<&str>) -> Prompt {
        let body = serde_json::to_value(&request).expect("request json");
        let model = body["body"]["model"]
            .as_str()
            .unwrap_or("test-model")
            .to_owned();
        let prompt = Prompt::new(request, model).expect("static prompt is valid");
        assert_eq!(prompt.variables, variables, "fixture variables are derived");
        prompt
    }

    /// The `support_prompt` version `1.0.0` Prompt Card in the `default` space
    /// wrapping `spec`, as the boundary tests store it.
    ///
    /// # Panics
    /// Panics when a static Card identity field is invalid.
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
            }]
            .into(),
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
        let ProviderRequest::AnthropicMessage(mut request) = anthropic_request("hello") else {
            unreachable!("helper returns Anthropic")
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
        let ProviderRequest::AnthropicMessage(mut request) = anthropic_request("hello") else {
            unreachable!("helper returns Anthropic")
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

    pub fn raw_request(provider: ProviderName) -> ProviderRequest {
        ProviderRequest::RawV1 {
            provider,
            body: serde_json::json!({
                "native": true,
                "nested": { "b": 2, "a": 1 }
            }),
        }
    }

    /// OpenAI Chat structured-output field for an `answer` object.
    pub fn json_schema_response_format() -> OpenAiResponseFormat {
        OpenAiResponseFormat::JsonSchema {
            json_schema: OpenAiJsonSchema {
                name: "answer".to_owned(),
                description: None,
                schema: serde_json::json!({
                    "type": "object",
                    "properties": { "answer": { "type": "string" } }
                })
                .as_object()
                .cloned(),
                strict: Some(true),
            },
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
}

#[cfg(test)]
mod prompt_codec_tests {
    use super::prompt_support::{prompt_card, prompt_spec};
    use crate::{CardLoadFormat, parse_card_bytes, serialize_card};

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
    fn from_extension_rejects_txt() {
        let err = CardLoadFormat::from_extension(Some("txt")).expect_err("txt rejected");

        assert_eq!(err.code(), "WYRD_PROMPT_400_LOADER_BAD_EXTENSION");
    }
}

#[cfg(test)]
mod prompt_construct_tests {
    use super::prompt_support::{
        anthropic_request, google_request, openai_chat_request, openai_responses_request,
        prompt_spec,
    };

    #[test]
    fn new_provider_variants_build() {
        for request in [
            openai_chat_request("hello"),
            openai_responses_request("hello"),
            anthropic_request("hello"),
            google_request("hello"),
        ] {
            let spec = prompt_spec(request, Vec::new());
            assert!(spec.parameters().is_empty());
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
        json_schema_response_format, mutate_first_text, openai_chat_request, prompt, prompt_spec,
    };
    use crate::PromptSpec;

    /// The hash follows the authored model and content, including a native
    /// structured-output field.
    ///
    /// # Panics
    /// Panics when a fixture Prompt is invalid or a mutation leaves the hash
    /// unchanged.
    #[test]
    fn hash_changes_on_model_request_and_response_format_mutations() {
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

        let mut changed_response = base_prompt;
        if let skald_spec::ProviderRequest::OpenAiChatCompletion(request) =
            &mut changed_response.request
        {
            request.response_format = Some(json_schema_response_format());
        }
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
mod prompt_authored_tests {
    //! Prompt Cards keep their authored portable or native spec through
    //! YAML and JSON round trips and refuse unknown providers, removed
    //! fields, and raw requests that are not an authored Prompt.

    use crate::envelope::{Card, CardKind, Spec};
    use crate::format;
    use skald_spec::{ProviderRequest, ResponseType};

    /// Wraps an authored spec body in a Prompt Card YAML document.
    fn card_yaml(spec: &str) -> String {
        let spec = spec
            .lines()
            .map(|line| format!("  {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("kind: Prompt\nmetadata:\n  name: test-prompt\n  version: 0.1.0\nspec:\n{spec}\n")
    }

    /// Decodes a Card and returns its authored spec as JSON after re-encoding.
    ///
    /// # Panics
    /// Panics when the Card does not decode, encode, or re-decode to itself.
    fn round_trip(yaml: &str) -> (Card, serde_json::Value) {
        let card: Card = format::yaml::from_str(yaml).expect("card decodes");
        let encoded = format::yaml::to_string(&card).expect("card encodes");
        let decoded: Card = format::yaml::from_str(&encoded).expect("card re-decodes");
        assert_eq!(decoded, card);
        let spec = serde_json::to_value(&card).expect("card json")["spec"].clone();
        (card, spec)
    }

    /// A portable Card compiles to the provider default format and serializes
    /// back to exactly the authored fields.
    ///
    /// # Panics
    /// Panics when the Card does not round-trip, compiles to another request,
    /// variables, or response type, or serializes other fields.
    #[test]
    fn portable_card_round_trips_as_authored() {
        let (card, spec) = round_trip(&card_yaml(
            "provider: openai\nmodel: gpt-4o\nsystem: You are {{persona}}.\nmessages:\n  - role: user\n    content: Summarize {{doc}}.\nsettings:\n  temperature: 0.2",
        ));
        assert_eq!(card.kind, CardKind::Prompt);
        let Spec::Prompt(prompt) = &card.spec else {
            panic!("expected Prompt spec");
        };
        assert!(matches!(
            prompt.prompt.request,
            ProviderRequest::OpenAiChatCompletion(_)
        ));
        assert_eq!(prompt.prompt.variables, vec!["persona", "doc"]);
        assert_eq!(prompt.prompt.response_type, ResponseType::Text);
        assert_eq!(
            spec,
            serde_json::json!({
                "provider": "openai",
                "model": "gpt-4o",
                "system": "You are {{persona}}.",
                "messages": [{"role": "user", "content": "Summarize {{doc}}."}],
                "settings": {"temperature": 0.2},
            })
        );
    }

    /// A native Card keeps its format and body verbatim.
    ///
    /// # Panics
    /// Panics when the Card does not round-trip or its format or body changes.
    #[test]
    fn native_card_round_trips_as_authored() {
        let (_, spec) = round_trip(&card_yaml(
            "provider: anthropic\nmodel: claude-sonnet-5\nformat: messages\nbody:\n  max_tokens: 64\n  messages:\n    - role: user\n      content:\n        - type: text\n          text: Hello",
        ));
        assert_eq!(spec["format"], "messages");
        assert_eq!(spec["body"]["max_tokens"], 64);
        assert!(spec["body"].get("model").is_none());
    }

    /// An authored Card that omits `apiVersion` deserializes as `wyrd/v1`
    /// and serializes the value explicitly.
    ///
    /// # Panics
    /// Panics when the Card does not deserialize, carries another version, or
    /// serializes without an explicit `apiVersion`.
    #[test]
    fn api_version_defaults_to_v1() {
        let (card, _) = round_trip(&card_yaml(
            "provider: openai\nmodel: gpt-4o\nmessages:\n  - role: user\n    content: Hello",
        ));
        assert_eq!(card.api_version.as_str(), "wyrd/v1");
        assert!(
            format::yaml::to_string(&card)
                .expect("card serializes")
                .contains("apiVersion: wyrd/v1")
        );
    }

    /// Unknown providers and removed or derived fields are refused, never
    /// ignored.
    ///
    /// # Panics
    /// Panics when any of the refused specs decodes.
    #[test]
    fn unknown_provider_and_removed_fields_refused() {
        for spec in [
            "provider: Bad Provider\nmodel: gpt-4o\nmessages:\n  - role: user\n    content: Hello",
            "provider: openai\nmodel: gpt-4o\nmessages:\n  - Hello",
            "provider: openai\nmodel: gpt-4o\nvariables: [x]\nmessages:\n  - role: user\n    content: Hello",
            "provider: openai\nmodel: gpt-4o\nmodel_settings: {temperature: 1}\nmessages:\n  - role: user\n    content: Hello",
            "provider: openai\nmodel: gpt-4o\nrequest: {}",
        ] {
            let result: Result<Card, _> = format::yaml::from_str(&card_yaml(spec));
            assert!(result.is_err(), "refused: {spec}");
        }
    }
}

#[cfg(test)]
mod prompt_envelope_invariants_tests {
    use super::prompt_support::{prompt, raw_request};
    use crate::PromptSpec;
    use skald_spec::ProviderName;

    /// A raw provider body has no authored Prompt format, so it cannot become
    /// a Prompt Card.
    ///
    /// # Panics
    /// Panics when the raw body is accepted or refused with another code.
    #[test]
    fn raw_request_is_not_an_authored_prompt() {
        let err = PromptSpec::new(prompt(raw_request(ProviderName::OpenAi), Vec::new()))
            .expect_err("raw body refused");

        assert_eq!(err.code(), "WYRD_PROMPT_400_INVALID");
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
            space: Some(SpaceName::new("default").expect("static space is valid")),
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
mod prompt_skald_boundary_tests {
    use super::prompt_support::{
        anthropic_with_system, openai_chat_request, prompt_card, prompt_spec,
    };
    use skald_spec::ProviderRequest;

    /// A stored Anthropic Prompt Card, read back from JSON and rendered,
    /// serializes to a body the gateway parses back to the identical request
    /// and bytes.
    ///
    /// # Panics
    /// Panics when the Card does not round-trip or render, or the parsed
    /// request or its bytes differ.
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
