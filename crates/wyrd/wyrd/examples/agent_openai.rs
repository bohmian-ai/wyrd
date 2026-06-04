//! Single-agent OpenAI example; run with `cargo run -p wyrd --example agent_openai --all-features`.

use wyrd::Agent;
use wyrd::agent::{OpenAiChatOptions, openai_chat};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    wyrd::init();

    let prompt = openai_chat(
        "gpt-5.4-nano-2026-03-17",
        OpenAiChatOptions {
            system: Some("You are a helpful assistant.".to_owned()),
            messages: vec!["Say hello in one sentence.".to_owned()],
            ..OpenAiChatOptions::default()
        },
    )?;

    let agent = Agent::new(prompt);
    let providers = skald_runtime::default_registry();
    let run = agent.run_with(providers.as_ref(), None, "hello").await?;

    println!("finish: {:?}", run.finish_reason);
    println!("output: {}", run.output);

    Ok(())
}
