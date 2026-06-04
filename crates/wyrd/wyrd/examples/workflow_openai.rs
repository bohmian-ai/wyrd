//! OpenAI sequential workflow example; run with `cargo run -p wyrd --example workflow_openai --all-features`.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wyrd::agent::{OpenAiChatOptions, ToolDef, ToolError, openai_chat};
use wyrd::{Agent, Workflow};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SearchInput {
    query: String,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
struct SearchOutput {
    results: Vec<String>,
}

fn web_search(input: SearchInput) -> Result<SearchOutput, ToolError> {
    if input.query.trim().is_empty() {
        return Err(ToolError::Invocation {
            detail: "query must not be empty".to_owned(),
            cause: None,
        });
    }
    Ok(SearchOutput {
        results: vec![
            format!("Result 1 for '{}'", input.query),
            format!("Result 2 for '{}'", input.query),
        ],
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let search_tool = ToolDef::function(
        "web_search",
        "Search the web and return a list of result snippets.",
        web_search,
    );

    let researcher_prompt = openai_chat(
        "gpt-4o-mini",
        OpenAiChatOptions {
            system: Some(
                "You are a concise researcher. Use the web_search tool to find 3 key facts about the topic, then summarise them."
                    .to_owned(),
            ),
            messages: vec!["Research: {{topic}}".to_owned()],
            variables: vec!["topic".to_owned()],
            ..OpenAiChatOptions::default()
        },
    )?;

    let writer_prompt = openai_chat(
        "gpt-4o-mini",
        OpenAiChatOptions {
            system: Some(
                "You are a concise writer. Turn the research notes into a two-sentence summary."
                    .to_owned(),
            ),
            messages: vec!["Write a summary from: {{research}}".to_owned()],
            variables: vec!["research".to_owned()],
            ..OpenAiChatOptions::default()
        },
    )?;

    let researcher = Agent::new(researcher_prompt)
        .name("researcher")
        .version("0.1.0")
        .with_tool(Arc::new(search_tool));
    let writer = Agent::new(writer_prompt).name("writer").version("0.1.0");

    let workflow =
        Workflow::sequential("research-and-write", [researcher, writer])?.with_version("0.1.0");
    let run = workflow.run("renewable energy storage").await?;

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
