# wyrd-sdk-python examples

Examples use the built-in `mock` provider so they run without credentials.

- `from_yaml.py` loads the `examples/workflows/code-review` bundle.
- `structured_pipeline.py` runs one Agent with structured output and typed
  callbacks.

Workflows are authored as YAML and loaded with `Workflow.from_path` or
`Workflow.from_yaml`; there is no Python builder.
