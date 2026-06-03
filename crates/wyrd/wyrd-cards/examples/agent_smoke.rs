use std::path::PathBuf;

use skald_prompt::{OpenAiChatOptions, openai_chat};
use wyrd_cards::agent::AgentWithMeta;
use wyrd_spec::reference::PromptRef;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut save_path = PathBuf::from("/tmp/planner_rust.yaml");
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--save" {
            if let Some(path) = args.next() {
                save_path = PathBuf::from(path);
            }
        }
    }

    let prompt = openai_chat(
        "gpt-4o-mini",
        OpenAiChatOptions {
            system: Some("Plan concise next steps.".to_owned()),
            messages: vec!["Plan the release for {{topic}}.".to_owned()],
            variables: vec!["topic".to_owned()],
            version: Some("0.3.0".to_owned()),
            ..OpenAiChatOptions::default()
        },
    )?;

    let agent = AgentWithMeta::builder()
        .name("planner-agent")
        .version("0.3.0")
        .space("research")
        .prompt(PromptRef::from(prompt.into_native()))
        .build()?;

    agent.save(save_path)?;
    Ok(())
}
