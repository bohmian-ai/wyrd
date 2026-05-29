mod prompt_support;

use prompt_support::{openai_chat_request, prompt_spec};
use wyrd_spec::PromptRef;
use wyrd_spec::envelope::CardKind;
use wyrd_spec::reference::CardRef;

#[test]
fn card_variant_roundtrip() {
    let reference = PromptRef::Card(CardRef {
        kind: CardKind::Prompt,
        name: "support_prompt".parse().expect("valid card name"),
        version: "1.0.0".parse().expect("valid version"),
        space: None,
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
