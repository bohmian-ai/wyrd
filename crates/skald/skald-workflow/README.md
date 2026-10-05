# skald-workflow

Local workflow authoring and explicit-binding DAG execution.

Dependencies order steps only. Each step binds its unresolved Prompt variables
to `input.<name>` or a dependency's `steps.<id>.output.text` /
`steps.<id>.output.structured[.<field>...]`, and the Workflow names its
outputs. The run returns the portable `WorkflowRun` snapshot.

```rust
use skald_workflow::{Workflow, WorkflowBinding};
use wyrd_spec::card::common::ParameterValue;

let bind = |source: &str| WorkflowBinding::new(source);
let wf = Workflow::sequential("research", vec![planner, writer])?
    .with_inputs([("topic".to_owned(), ParameterValue::Str(String::new()))].into())?
    .with_step_inputs("planner", [("topic".to_owned(), bind("input.topic")?)].into())?
    .with_step_inputs("writer", [("notes".to_owned(), bind("steps.planner.output.text")?)].into())?
    .with_outputs([("brief".to_owned(), bind("steps.writer.output.text")?)].into())?;
let input = serde_json::Map::from_iter([("topic".to_owned(), "climate change".into())]);
let run = wyrd_runtime::runtime().block_on(wf.run(input))?;
println!("{:?}: {:?}", run.status, run.outputs);
# Ok::<(), Box<dyn std::error::Error>>(())
```

See `examples/rust/workflow_*.rs` and the docsite guide "Build an Agent Workflow".
