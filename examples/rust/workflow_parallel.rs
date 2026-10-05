//! Parallel workflow: two researchers fan into one synthesizer.

use serde_json::json;
use skald_agent::Agent;
use skald_workflow::Workflow;

mod common;
use common::{bindings, mock_registry, plan_prompt, string_input, write_prompt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let researcher_a = Agent::new(plan_prompt()).name("researcher_a");
    let researcher_b = Agent::new(plan_prompt()).name("researcher_b");
    let synthesizer = Agent::new(write_prompt()).name("synthesizer");

    let wf = Workflow::parallel(
        "parallel_research",
        vec![researcher_a.clone(), researcher_b.clone()],
    )?
    .add_after(synthesizer, vec!["researcher_a", "researcher_b"])?
    .with_inputs(string_input("topic"))?
    .with_step_inputs("researcher_a", bindings(&[("topic", "input.topic")]))?
    .with_step_inputs("researcher_b", bindings(&[("topic", "input.topic")]))?
    .with_step_inputs(
        "synthesizer",
        bindings(&[("summary", "steps.researcher_a.output.structured.summary")]),
    )?
    .with_outputs(bindings(&[
        ("brief", "steps.synthesizer.output.text"),
        ("b_plan", "steps.researcher_b.output.structured"),
    ]))?;

    let providers = mock_registry(&[
        r#"{"summary":"A summary","steps":["a1","a2"]}"#,
        r#"{"summary":"B summary","steps":["b1","b2"]}"#,
        "Synthesized brief",
    ]);
    let run = wf
        .run_with(
            &providers,
            serde_json::Map::from_iter([("topic".to_owned(), json!("the Rust borrow checker"))]),
        )
        .await?;
    println!("steps: {}", run.steps.len());
    println!("outputs: {:#?}", run.outputs);
    Ok(())
}
