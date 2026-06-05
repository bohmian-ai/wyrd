//! Load a workflow from YAML and run it.

use std::collections::HashMap;

use skald_workflow::{Workflow, WorkflowInput};

fn main() -> anyhow::Result<()> {
    let wf = Workflow::load(
        "examples/workflows/research.yaml",
        skald_tool::default_registry(),
        skald_agent::default_prompt_resolver(),
    )?;
    let run = wyrd_runtime::runtime().block_on(wf.run(WorkflowInput::from(HashMap::from([(
        "topic".to_owned(),
        "climate change".to_owned(),
    )]))))?;
    println!("steps completed: {}", run.tasks.len());
    println!("parameters: {:?}", run.parameters);
    if let Some(text) = run.final_output.as_deref() {
        println!("final output: {text}");
    }
    Ok(())
}
