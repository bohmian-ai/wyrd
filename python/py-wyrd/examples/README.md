# py-wyrd examples

Examples use the built-in `mock` provider so they run without credentials.

- `from_yaml.py` loads `workflows/research.yaml`.
- `from_builder.py` builds a workflow in Python.
- `parallel.py` runs two researcher agents in parallel, then fans into one
  synthesizer step.
- `structured_pipeline.py` demonstrates structured output and typed callbacks.
- `with_observer.py` attaches `OtelObserver` and a custom observer through
  `Workflow.sequential(..., observers=[...])`.
