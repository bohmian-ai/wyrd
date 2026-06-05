//! Build a workflow with the Rust API and run it.

use std::collections::HashMap;

use skald_agent::Agent;
use skald_workflow::{Workflow, WorkflowInput};

mod common;
use common::{plan_prompt, write_prompt};

fn main() -> anyhow::Result<()> {
    let planner = Agent::new(plan_prompt()).name("planner");
    let writer = Agent::new(write_prompt()).name("writer");
    let wf = Workflow::sequential("research", vec![planner, writer])?;
    wf.save("/tmp/research.yaml")?;

    let run = wyrd_runtime::runtime().block_on(wf.run(WorkflowInput::from(HashMap::from([(
        "topic".to_owned(),
        "the Rust borrow checker".to_owned(),
    )]))))?;
    println!("steps completed: {}", run.tasks.len());
    println!("parameters: {:?}", run.parameters);
    if let Some(text) = run.final_output.as_deref() {
        println!("final: {text}");
    }
    Ok(())
}
