mod prompt_support;

use prompt_support::{prompt_card, prompt_spec, raw_body_text, raw_request};
use skald_spec::ProviderName;
use wyrd_spec::{CardLoadFormat, parse_card_bytes, serialize_card};

#[test]
fn parse_serialize_json_card_roundtrip() {
    let card = prompt_card(prompt_spec(
        prompt_support::openai_chat_request("hello"),
        Vec::new(),
    ));
    let bytes = serialize_card(CardLoadFormat::Json, &card).expect("serialize");
    let decoded = parse_card_bytes(CardLoadFormat::Json, &bytes).expect("parse");

    assert_eq!(decoded, card);
}

#[test]
fn parse_serialize_yaml_card_roundtrip() {
    let card = prompt_card(prompt_spec(
        prompt_support::openai_chat_request("hello"),
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
        wyrd_spec::envelope::Spec::Prompt(spec) => raw_body_text(&spec.prompt.request),
        _ => None,
    };
    let bytes = serialize_card(CardLoadFormat::Json, &card).expect("serialize");
    let decoded = parse_card_bytes(CardLoadFormat::Json, &bytes).expect("parse");
    let decoded_body = match &decoded.spec {
        wyrd_spec::envelope::Spec::Prompt(spec) => raw_body_text(&spec.prompt.request),
        _ => None,
    };

    assert_eq!(decoded_body, original_body);
}

#[test]
fn from_extension_rejects_txt() {
    let err = CardLoadFormat::from_extension(Some("txt")).expect_err("txt rejected");

    assert_eq!(err.code(), "WYRD_PROMPT_400_LOADER_BAD_EXTENSION");
}
