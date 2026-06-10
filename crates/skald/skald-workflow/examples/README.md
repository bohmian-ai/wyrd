# skald-workflow examples

These examples show the local workflow authoring surface:

- `from_builder.rs` builds a two-step workflow in Rust.
- `from_yaml.rs` loads `examples/workflows/research.yaml`.
- `parallel.rs` runs two researcher agents in parallel, then fans into one
  synthesizer step.
- `with_observer.rs` attaches `OtelObserver` and a custom token counter with
  `Workflow::with_observers`.

Build them with `cargo build -p skald-workflow --examples --all-features`.
