# Rust examples

Run these examples from the repository root:

```bash
cargo run -p wyrd-rust-examples --bin workflow_parallel
```

Mock-provider examples run without credentials:

- `agent_smoke.rs` builds an Agent card and saves local YAML.
- `workflow_from_builder.rs` builds and runs a two-step workflow.
- `workflow_from_yaml.rs` loads `workflows/research.yaml`.
- `workflow_parallel.rs` runs two researchers in parallel, then fans into one
  synthesizer step.
- `workflow_with_observer.rs` attaches `OtelObserver` and a custom token
  counter.

Provider examples require the matching provider credentials:

- `agent_openai.rs`
- `workflow_openai.rs`
- `workflow_anthropic.rs`
- `workflow_gemini.rs`
- `workflow_gateway.rs`
- `workflow_structured_output.rs`
- `tracing_stdout.rs`
