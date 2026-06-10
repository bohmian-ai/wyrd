# skald-agent

Runnable Wyrd agent loop.

```rust
use skald_agent::Agent;
use skald_prompt::{OpenAiChatOptions, openai_chat};

let prompt = openai_chat("gpt-4o-mini", OpenAiChatOptions {
    messages: vec!["Help with ${input}".to_owned()],
    ..Default::default()
})?;
let agent = Agent::new(prompt).name("assistant");
let run = wyrd_runtime::runtime().block_on(agent.run("draft the doc"))?;
println!("{}", run.output);
# Ok::<(), Box<dyn std::error::Error>>(())
```
