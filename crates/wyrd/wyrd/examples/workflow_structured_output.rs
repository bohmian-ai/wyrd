//! Two-step structured-output workflow against the live OpenAI API.
//!
//! Run with:
//! `cargo run -p wyrd --example workflow_structured_output --all-features`.

use serde_json::json;
use wyrd::agent::{Agent, OpenAiChatOptions, Workflow, openai_chat};
use wyrd::prompt::ResponseFormat;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wyrd::init();

    let planner_schema = json!({
        "type": "object",
        "properties": {
            "summary": {"type": "string"},
            "steps": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["summary", "steps"],
        "additionalProperties": false
    });

    let planner = Agent::new(openai_chat(
        "gpt-5.4-nano-2026-03-17",
        OpenAiChatOptions {
            system: Some("You produce structured plans.".into()),
            messages: vec!["Plan: ${topic}".into()],
            output: Some(ResponseFormat::json_schema("plan", planner_schema)?),
            ..OpenAiChatOptions::default()
        },
    )?)
    .name("planner");

    let writer = Agent::new(openai_chat(
        "gpt-5.4-nano-2026-03-17",
        OpenAiChatOptions {
            system: Some("You write 2-sentence briefs.".into()),
            messages: vec!["Write a brief from this summary: ${summary}".into()],
            ..OpenAiChatOptions::default()
        },
    )?)
    .name("writer");

    let wf = Workflow::sequential("demo", [planner, writer])?;
    let run = wf.run(json!({"topic": "the Rust borrow checker"})).await?;

    println!("parameters: {:#?}", run.parameters);
    println!("final: {}", run.final_output.as_deref().unwrap_or("(none)"));
    Ok(())
}
