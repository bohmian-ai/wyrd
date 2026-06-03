use std::sync::Arc;

use serde_json::json;
use skald_agent::{Agent, RunConfig};
use skald_prompt::Prompt;
use skald_spec::{
    Prompt as SpecPrompt, ProviderRequest, ResponseType,
    wire::openai_chat::{
        OpenAiChatMessage, OpenAiChatRequest, OpenAiChatSettings, OpenAiMessageContent,
    },
};
use skald_tool::ToolDef;

fn test_prompt(model: &str) -> Arc<Prompt> {
    let request = ProviderRequest::OpenAiChatCompletion(OpenAiChatRequest {
        model: model.to_owned(),
        messages: vec![OpenAiChatMessage {
            role: "system".to_owned(),
            content: Some(OpenAiMessageContent::Text("system".to_owned())),
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
        settings: OpenAiChatSettings::default(),
    });
    Arc::new(Prompt::from_native(
        SpecPrompt::new(request, model, None, ResponseType::Text).expect("test prompt builds"),
    ))
}

fn tool_def(name: &str) -> ToolDef {
    ToolDef::new(
        name,
        format!("{name} test tool"),
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        }),
    )
    .expect("test tool definition validates")
}

#[test]
fn agent_new_has_default_public_fields() {
    let prompt = test_prompt("gpt-4o");
    let agent = Agent::from_resolved("planner", prompt);

    assert_eq!(agent.id, "planner");
    assert_eq!(agent.run_config, RunConfig::default());
    assert!(agent.tool_names().is_empty());
}

#[test]
fn agent_add_tool_appends_to_cache_order() {
    let first = Arc::new(tool_def("t1"));
    let second = Arc::new(tool_def("t2"));

    let agent = Agent::from_resolved("a", test_prompt("gpt-4o"))
        .add_tool(first)
        .add_tool(second);

    assert_eq!(agent.tool_names(), vec!["t1".to_owned(), "t2".to_owned()]);
}

#[test]
fn agent_set_tools_replaces_full_list() {
    let first = Arc::new(tool_def("t1"));
    let second = Arc::new(tool_def("t2"));
    let third = Arc::new(tool_def("t3"));

    let agent = Agent::from_resolved("a", test_prompt("gpt-4o"))
        .add_tool(first)
        .set_tools(vec![second, third]);

    assert_eq!(agent.tool_names(), vec!["t2".to_owned(), "t3".to_owned()]);
}

#[test]
fn agent_with_prompt_returns_new_agent_with_same_tools() {
    let original_prompt = test_prompt("gpt-4o");
    let replacement_prompt = test_prompt("gpt-4o-mini");
    let tool = Arc::new(tool_def("echo"));
    let agent = Agent::from_resolved("a", original_prompt.clone()).add_tool(tool);

    let replaced = agent.clone().with_prompt(replacement_prompt.clone());

    assert_eq!(agent.tool_names(), replaced.tool_names());
    assert!(Arc::ptr_eq(&agent.prompt, &original_prompt));
    assert!(Arc::ptr_eq(&replaced.prompt, &replacement_prompt));
}

#[test]
fn agent_with_run_config_preserves_config() {
    let config = RunConfig {
        max_iterations: 3,
        ..Default::default()
    };

    let agent = Agent::from_resolved("a", test_prompt("gpt-4o")).with_run_config(config.clone());

    assert_eq!(agent.run_config, config);
}

#[test]
fn agent_tool_names_reflect_tool_internal_name() {
    let tool = Arc::new(tool_def("echo"));

    let agent = Agent::from_resolved("a", test_prompt("gpt-4o")).add_tool(tool);

    assert_eq!(agent.tool_names(), vec!["echo".to_owned()]);
}

#[test]
fn agent_clone_bumps_prompt_arc() {
    let prompt = test_prompt("gpt-4o");
    let agent = Agent::from_resolved("a", prompt.clone())
        .add_tool(Arc::new(tool_def("t1")))
        .add_tool(Arc::new(tool_def("t2")))
        .add_tool(Arc::new(tool_def("t3")));
    let prompt_count = Arc::strong_count(&prompt);

    let cloned = agent.clone();

    assert_eq!(Arc::strong_count(&prompt), prompt_count + 1);
    assert_eq!(cloned.tool_names(), agent.tool_names());
}
