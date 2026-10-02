//! Build a workflow with the Rust API and run it.

use serde_json::json;
use skald_agent::Agent;
use skald_workflow::Workflow;

mod common;
use common::{bindings, mock_registry, plan_prompt, string_input, write_prompt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let planner = Agent::new(plan_prompt()).name("planner");
    let writer = Agent::new(write_prompt()).name("writer");
    let wf = Workflow::sequential("research", vec![planner, writer])?
        .with_version("0.1.0")
        .with_inputs(string_input("topic"))?
        .with_step_inputs("planner", bindings(&[("topic", "input.topic")]))?
        .with_step_inputs(
            "writer",
            bindings(&[("summary", "steps.planner.output.structured.summary")]),
        )?
        .with_outputs(bindings(&[
            ("brief", "steps.writer.output.text"),
            ("plan", "steps.planner.output.structured"),
        ]))?;
    wf.validate()?;
    wf.save("/tmp/research.yaml")?;

    let providers = mock_registry(&[
        r#"{"summary":"mock summary","steps":["read","write"]}"#,
        "Final local brief",
    ]);
    let run = wf
        .run_with(
            &providers,
            serde_json::Map::from_iter([("topic".to_owned(), json!("the Rust borrow checker"))]),
        )
        .await?;
    println!("status: {:?}", run.status);
    println!("outputs: {:#?}", run.outputs);
    Ok(())
}
