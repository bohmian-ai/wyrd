use std::collections::BTreeMap;

use skald_spec::wire::openai_chat::OpenAiMessageContent;
use skald_spec::{
    OpenAiChatMessage, OpenAiChatRequest, OpenAiChatSettings, Prompt, ProviderRequest, ResponseType,
};
use wyrd_spec::card::prompt::PromptSpec;
use wyrd_spec::envelope::{Card, CardKind, Metadata, Relationships, Spec};
use wyrd_spec::format;
use wyrd_spec::ids::CardName;
use wyrd_spec::metadata::{AnnotationKey, AnnotationValue, LabelKey, LabelValue, MetadataError};
use wyrd_spec::version::{ApiVersion, VersionBlock};

#[test]
fn labels_and_annotations_round_trip_as_string_maps() {
    let mut labels = BTreeMap::new();
    labels.insert(
        LabelKey::new("acme.com/domain").expect("static label key is valid"),
        LabelValue::new("churn").expect("static label value is valid"),
    );
    let mut annotations = BTreeMap::new();
    annotations.insert(
        AnnotationKey::new("acme.com/source-table").expect("static annotation key is valid"),
        AnnotationValue::new("warehouse.customer_churn").expect("static annotation value is valid"),
    );
    let card = Card {
        api_version: ApiVersion::v1(),
        kind: CardKind::Prompt,
        metadata: Metadata {
            name: CardName::new("support_prompt").expect("static card name is valid"),
            version: VersionBlock::parse("1.0.0").expect("static version is valid"),
            space: None,
            uid: None,
            labels,
            annotations,
            spec_hash: None,
            artifact_hash: None,
        },
        spec: Spec::Prompt(prompt_spec()),
        relationships: Relationships::default(),
        status: None,
    };

    let json = serde_json::to_string(&card).expect("card should serialize");
    assert!(json.contains(r#""labels":{"acme.com/domain":"churn"}"#));
    assert!(json.contains(r#""annotations":{"acme.com/source-table":"warehouse.customer_churn"}"#));
    let decoded: Card = serde_json::from_str(&json).expect("card should deserialize");
    assert_eq!(decoded, card);

    let yaml = format::yaml::to_string(&card).expect("card should serialize to yaml");
    let decoded: Card = format::yaml::from_str(&yaml).expect("card should deserialize from yaml");
    assert_eq!(decoded, card);
}

fn prompt_spec() -> PromptSpec {
    PromptSpec::new(Prompt {
        request: ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
            model: "gpt-4o".to_owned(),
            messages: vec![OpenAiChatMessage {
                role: "user".to_owned(),
                content: Some(OpenAiMessageContent::Text("Answer carefully.".to_owned())),
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
        }),
        model: "gpt-4o".to_owned(),
        version: None,
        variables: Vec::new(),
        media_variables: Vec::new(),
        response_type: ResponseType::Text,
    })
    .expect("static prompt spec is valid")
}

#[test]
fn label_keys_reject_invalid_grammar() {
    for value in [
        "",
        "/name",
        "prefix/",
        "bad_prefix!/name",
        "acme.com/-name",
        "acme.com/name-",
        "acme.com/na me",
    ] {
        assert_eq!(LabelKey::new(value), Err(MetadataError::InvalidKey));
    }
    assert_eq!(
        LabelKey::new(format!("{}a", "a".repeat(63))),
        Err(MetadataError::InvalidKey)
    );
}

#[test]
fn label_values_reject_invalid_values() {
    assert_eq!(
        LabelValue::new("has space"),
        Err(MetadataError::InvalidLabelValue)
    );
    assert_eq!(
        LabelValue::new("-bad"),
        Err(MetadataError::InvalidLabelValue)
    );
    assert_eq!(
        LabelValue::new(format!("{}a", "a".repeat(63))),
        Err(MetadataError::InvalidLabelValue)
    );
    assert!(LabelValue::new("").is_ok());
}

#[test]
fn annotations_reject_oversized_values() {
    assert_eq!(
        AnnotationValue::new(format!("{}a", "a".repeat(4096))),
        Err(MetadataError::InvalidAnnotationValue)
    );
}

#[test]
fn user_metadata_rejects_reserved_prefixes_and_secret_like_values() {
    assert!(AnnotationKey::new("wyrd.io/readme").is_ok());
    assert_eq!(
        AnnotationKey::new_user("wyrd.io/readme"),
        Err(MetadataError::ReservedKey)
    );
    assert_eq!(
        AnnotationKey::new_user("acme.com/api-token"),
        Err(MetadataError::SecretLikeValue)
    );
    assert_eq!(
        AnnotationValue::new_user("Bearer abc123"),
        Err(MetadataError::SecretLikeValue)
    );
}
