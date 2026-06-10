//! Parallel workflow: two researchers fan into one synthesizer.

use std::collections::HashMap;

use skald_agent::Agent;
use skald_workflow::{Workflow, WorkflowInput};

mod common;
use common::{mock_registry, plan_prompt, write_prompt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let researcher_a = Agent::new(plan_prompt()).name("researcher_a");
    let researcher_b = Agent::new(plan_prompt()).name("researcher_b");
    let synthesizer = Agent::new(write_prompt()).name("synthesizer");

    let wf = Workflow::parallel(
        "parallel_research",
        vec![researcher_a.clone(), researcher_b.clone()],
    )?
    .add_after(synthesizer, vec!["researcher_a", "researcher_b"])?;

    let providers = mock_registry(&[
        r#"{"summary":"A summary","steps":["a1","a2"]}"#,
        r#"{"summary":"B summary","steps":["b1","b2"]}"#,
        "Synthesized brief",
    ]);
    let run = wf
        .run_with(
            &providers,
            WorkflowInput::from(HashMap::from([(
                "topic".to_owned(),
                "the Rust borrow checker".to_owned(),
            )])),
        )
        .await?;
    println!("steps: {}", run.tasks.len());
    println!("final: {:?}", run.final_output);
    Ok(())
}
