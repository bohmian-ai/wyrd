use wyrd_spec::card::agent::{AgentRunConfigSpec, AgentSpec};
use wyrd_spec::envelope::CardKind;
use wyrd_spec::reference::{CardRef, PromptRef};

fn prompt() -> skald_spec::Prompt {
    skald_spec::Prompt::new(
        skald_spec::ProviderRequest::OpenAiChatCompletion(skald_spec::OpenAiChatRequest {
            model: "gpt-4o-mini".to_owned(),
            messages: vec![skald_spec::OpenAiChatMessage {
                role: "user".to_owned(),
                content: Some(skald_spec::wire::openai_chat::OpenAiMessageContent::Text(
                    "plan".to_owned(),
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
        }),
        "gpt-4o-mini",
        None,
        skald_spec::ResponseType::Text,
    )
    .expect("static prompt is valid")
}

#[test]
fn agent_spec_inline_prompt_roundtrips() {
    let spec = AgentSpec {
        prompt: PromptRef::from(prompt()),
        tool_names: vec!["search_docs".to_owned()],
        run_config: AgentRunConfigSpec {
            max_iterations: Some(7),
            tool_concurrency_cap: Some(2),
            session_recent_limit: Some(5),
            timeout_ms: Some(1_500),
        },
    };

    let json = serde_json::to_string(&spec).expect("serialize");
    let decoded: AgentSpec = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(decoded, spec);
    assert!(json.contains("session_recent_limit"));
    assert!(!json.contains(r#""kind":"inline""#));
    assert!(!json.contains(r#""value":"#));
}

#[test]
fn agent_spec_card_prompt_uses_single_version_field() {
    let spec = AgentSpec {
        prompt: PromptRef::from(CardRef {
            kind: CardKind::Prompt,
            name: "planner-prompt".parse().expect("valid card name"),
            version: "0.3.0".parse().expect("valid version"),
            space: Some("research".parse().expect("valid space")),
            uid: None,
        }),
        tool_names: Vec::new(),
        run_config: AgentRunConfigSpec::default(),
    };

    let yaml = serde_yaml::to_string(&spec).expect("serialize");
    let decoded: AgentSpec = serde_yaml::from_str(&yaml).expect("deserialize");

    assert_eq!(decoded, spec);
    assert!(yaml.contains("version: 0.3.0"));
    assert!(yaml.contains("kind: Prompt"));
    assert!(!yaml.contains("kind: card"));
    assert!(!yaml.contains("value:"));
    assert!(!yaml.contains(&format!("{}{}", "version", "_req")));
    assert!(!yaml.contains(&format!("{}{}", "version: ", "^")));
}
