use std::path::PathBuf;

use wyrd_cards::prompt::PromptCard;
use wyrd_cards::prompt::io::{read_card_file, write_card_file};
use wyrd_spec::envelope::Spec;

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "wyrd_prompt_card_s11_{}_{}",
        std::process::id(),
        name
    ))
}

fn prompt_card() -> PromptCard {
    let prompt = skald_spec::Prompt::new(
        skald_spec::ProviderRequest::RawV1 {
            provider: skald_spec::ProviderName::Custom("unit".to_owned()),
            body: serde_json::value::RawValue::from_string(r#"{"messages":["hello"]}"#.to_owned())
                .expect("static raw JSON is valid"),
        },
        "unit-model",
        None,
        skald_spec::ResponseType::Text,
    )
    .expect("static prompt is valid");
    let mut card = PromptCard::from_native_prompt(prompt);
    "growth".clone_into(&mut card.space);
    "lead-scoring".clone_into(&mut card.name);
    "1.2.3".clone_into(&mut card.version);
    card
}

fn typed_prompt_card() -> PromptCard {
    let request = skald_spec::OpenAiChatRequest {
        model: "gpt-4o-mini".to_owned(),
        messages: vec![skald_spec::OpenAiChatMessage {
            role: "user".to_owned(),
            content: Some(skald_spec::wire::openai_chat::OpenAiMessageContent::Text(
                "hello".to_owned(),
            )),
            name: None,
            tool_calls: None,
            tool_call_id: None,
            refusal: None,
        }],
        response_format: None,
        stream: None,
        stream_options: None,
        tools: None,
        tool_choice: None,
        parallel_tool_calls: None,
        settings: skald_spec::OpenAiChatSettings::default(),
    };
    let prompt = skald_spec::Prompt::new(
        skald_spec::ProviderRequest::OpenAiChatCompletion(request),
        "gpt-4o-mini",
        None,
        skald_spec::ResponseType::Text,
    )
    .expect("static typed prompt is valid");
    PromptCard::from_native_prompt(prompt)
}

#[test]
fn json_write_read_round_trip() {
    let path = temp_path("round_trip.json");
    let card = prompt_card().to_card().expect("card is valid");

    write_card_file(&card, &path).expect("write succeeds");
    let loaded = read_card_file(&path).expect("read succeeds");

    assert_eq!(loaded, card);
    let _ = std::fs::remove_file(path);
}

#[test]
fn yaml_write_read_round_trip() {
    let path = temp_path("round_trip.yaml");
    let card = typed_prompt_card().to_card().expect("card is valid");

    write_card_file(&card, &path).expect("write succeeds");
    let loaded = read_card_file(&path).expect("read succeeds");

    assert_eq!(loaded, card);
    let _ = std::fs::remove_file(path);
}

#[test]
fn yaml_prompt_card_envelope_with_model_settings_loads() {
    let path = temp_path("authored_card.yaml");
    std::fs::write(
        &path,
        r#"
apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: yaml-prompt
  version: 0.1.0
spec:
  type: Prompt
  prompt:
    request:
      model: gpt-4o
      messages:
        - role: user
          content: hello
      seed: 123
      future_knob:
        enabled: true
    model: gpt-4o
"#,
    )
    .expect("write fixture");

    let loaded = read_card_file(&path).expect("yaml prompt card loads");
    let card = PromptCard::from_card(loaded).expect("prompt card holder loads");
    let skald_spec::ProviderRequest::OpenAiChatCompletion(request) = card.metadata.prompt.request
    else {
        panic!("loaded prompt should be OpenAI chat");
    };

    assert_eq!(request.settings.seed, Some(123));
    assert_eq!(
        request.settings.extra["future_knob"],
        serde_json::json!({ "enabled": true })
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn txt_extension_returns_loader_bad_extension() {
    let path = temp_path("bad.txt");
    let card = prompt_card().to_card().expect("card is valid");

    let error = write_card_file(&card, &path).expect_err("bad extension fails");

    assert_eq!(error.code(), "WYRD_PROMPT_400_LOADER_BAD_EXTENSION");
}

#[test]
fn missing_file_returns_loader_io() {
    let path = temp_path("missing.json");
    let _ = std::fs::remove_file(&path);

    let error = read_card_file(&path).expect_err("missing file fails");

    assert_eq!(error.code(), "WYRD_PROMPT_500_LOADER_IO");
}

#[test]
fn raw_v1_prompt_body_preserves_provider_bytes() {
    let path = temp_path("raw_v1.json");
    let raw = r#"{"a":[true,{"nested":"value"}],"z":2}"#;
    let prompt = skald_spec::Prompt::new(
        skald_spec::ProviderRequest::RawV1 {
            provider: skald_spec::ProviderName::Custom("raw-provider".to_owned()),
            body: serde_json::value::RawValue::from_string(raw.to_owned())
                .expect("static raw JSON is valid"),
        },
        "raw-model",
        None,
        skald_spec::ResponseType::Text,
    )
    .expect("static prompt is valid");
    let card = PromptCard::from_native_prompt(prompt)
        .to_card()
        .expect("card is valid");

    write_card_file(&card, &path).expect("write succeeds");
    let loaded = read_card_file(&path).expect("read succeeds");

    let Spec::Prompt(spec) = loaded.spec else {
        panic!("loaded card is a Prompt spec");
    };
    let skald_spec::ProviderRequest::RawV1 { body, .. } = spec.prompt.request else {
        panic!("loaded prompt is RawV1");
    };
    assert_eq!(body.get(), raw);
    let _ = std::fs::remove_file(path);
}
