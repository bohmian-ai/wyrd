//! OpenAI gateway workflow example; run with `cargo run -p wyrd-rust-examples --bin workflow_gateway`.

use std::sync::Arc;

use wyrd::agent::{OpenAiChatOptions, ProviderRegistry, WorkflowBinding, openai_chat};
use wyrd::{Agent, Workflow};
use wyrd_spec::card::common::ParameterValue;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let gateway_url = std::env::var("GATEWAY_BASE_URL")
        .or_else(|_| std::env::var("OPENAI_BASE_URL"))
        .unwrap_or_else(|_| "https://api.openai.com/v1".to_owned());

    let prompt = openai_chat(
        "gpt-4o-mini",
        OpenAiChatOptions {
            system: Some("You are a helpful assistant.".to_owned()),
            messages: vec!["Summarise: {{topic}}".to_owned()],
            variables: vec!["topic".to_owned()],
            ..OpenAiChatOptions::default()
        },
    )?;
    let gateway_registry = Arc::new(ProviderRegistry::for_provider(
        &prompt.native().request.provider(),
        gateway_url,
        None::<String>,
    )?);

    let researcher = Agent::new(prompt.clone())
        .name("researcher")
        .version("0.1.0")
        .with_provider_registry(gateway_registry);
    let writer = Agent::new(prompt).name("writer").version("0.1.0");

    let binding = |source: &str| WorkflowBinding::new(source);
    let workflow = Workflow::sequential("gateway-demo", [researcher, writer])?
        .with_version("0.1.0")
        .with_inputs([("input".to_owned(), ParameterValue::Str(String::new()))].into())?
        .with_step_inputs(
            "researcher",
            [("topic".to_owned(), binding("input.input")?)].into(),
        )?
        .with_step_inputs(
            "writer",
            [("topic".to_owned(), binding("steps.researcher.output.text")?)].into(),
        )?
        .with_outputs([("summary".to_owned(), binding("steps.writer.output.text")?)].into())?;
    let run = workflow.run("renewable energy").await?;

    println!("status: {:?}", run.status);
    println!("outputs: {:#?}", run.outputs);

    Ok(())
}
