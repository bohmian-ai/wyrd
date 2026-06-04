//! OpenAI gateway workflow example; run with `cargo run -p wyrd --example workflow_gateway --all-features`.

use std::sync::Arc;

use wyrd::agent::{OpenAiChatOptions, ProviderRegistry, openai_chat};
use wyrd::{Agent, Workflow};

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

    let workflow =
        Workflow::sequential("gateway-demo", [researcher, writer])?.with_version("0.1.0");
    let run = workflow.run("renewable energy").await?;

    println!("steps completed: {}", run.tasks.len());
    if let Some(text) = run
        .result()
        .and_then(|outcome| outcome.result.as_ref())
        .and_then(|result| result.adapter().text())
    {
        println!("final output: {text}");
    }

    Ok(())
}
