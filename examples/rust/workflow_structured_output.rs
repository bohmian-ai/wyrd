//! Two-step structured-output workflow against the live OpenAI API.
//!
//! Run with:
//! `cargo run -p wyrd-rust-examples --bin workflow_structured_output`.

use serde_json::json;
use wyrd::agent::{Agent, OpenAiChatOptions, Workflow, WorkflowBinding, openai_chat};
use wyrd::prompt::ResponseFormat;
use wyrd_spec::card::common::ParameterValue;

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

    let binding = |source: &str| WorkflowBinding::new(source);
    let wf = Workflow::sequential("demo", [planner, writer])?
        .with_inputs([("topic".to_owned(), ParameterValue::Str(String::new()))].into())?
        .with_step_inputs(
            "planner",
            [("topic".to_owned(), binding("input.topic")?)].into(),
        )?
        .with_step_inputs(
            "writer",
            [(
                "summary".to_owned(),
                binding("steps.planner.output.structured.summary")?,
            )]
            .into(),
        )?
        .with_outputs(
            [
                (
                    "plan".to_owned(),
                    binding("steps.planner.output.structured")?,
                ),
                ("brief".to_owned(), binding("steps.writer.output.text")?),
            ]
            .into(),
        )?;
    let input =
        serde_json::Map::from_iter([("topic".to_owned(), json!("the Rust borrow checker"))]);
    let run = wf.run(input).await?;

    println!("status: {:?}", run.status);
    println!("outputs: {:#?}", run.outputs);
    Ok(())
}
