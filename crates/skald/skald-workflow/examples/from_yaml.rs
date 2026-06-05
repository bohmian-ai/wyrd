//! Load a workflow definition from YAML and inspect it.

use skald_workflow::Workflow;

fn main() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("workflows")
        .join("research.yaml");
    let wf = Workflow::load(
        path,
        skald_tool::default_registry(),
        skald_agent::default_prompt_resolver(),
    )?;
    println!("workflow: {}", wf.name().unwrap_or("anonymous"));
    println!("steps: {:?}", wf.steps());
    Ok(())
}
