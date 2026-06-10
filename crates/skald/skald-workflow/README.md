# skald-workflow

Local workflow authoring and DAG execution.

```rust
use skald_workflow::Workflow;

let wf = Workflow::sequential("research", vec![planner, writer])?;
let run = wyrd_runtime::runtime().block_on(wf.run("climate change"))?;
println!("steps: {}", run.tasks.len());
if let Some(outcome) = run.result() {
    println!("{:?}", outcome.status);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```

See `examples/` and the docsite guide "Build an Agent Workflow".
