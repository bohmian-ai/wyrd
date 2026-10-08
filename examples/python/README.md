# Python examples

Run these examples from a Wyrd checkout after installing the Python development
environment:

```bash
mise run py:setup
mise run examples:python:datacard
```

Expected output:

```text
saved_card_json: true
saved_data_file: data/data.parquet
loaded_rows: 3
loaded_columns: customer_id,churned,segment
labels: domain=customer,stage=example
annotation_source: examples/python/datacard_local_workflow.py
```

The DataCard example uses pandas because it is part of the Wyrd Python
development setup. It writes into a temporary directory and removes the local
files before exiting.

## Workflow examples

Each Workflow example loads a YAML Workflow bundle from `examples/workflows/`
with `Workflow.from_path` and runs it. Workflows are authored as YAML; the
Python SDK has no Workflow builder.

| Example | Workflow bundle | Needs |
|---|---|---|
| `workflow_openai.py` (`mise run examples:python:workflow:openai`) | `research-and-write/openai/` | `OPENAI_API_KEY` |
| `workflow_anthropic.py` (`mise run examples:python:workflow:anthropic`) | `research-and-write/anthropic/` | `ANTHROPIC_API_KEY` |
| `workflow_gemini.py` (`mise run examples:python:workflow:gemini`) | `research-and-write/gemini/` | Gemini credentials |
| `workflow_gateway.py` (`mise run examples:python:workflow:gateway`) | `gateway-demo/` | `OPENAI_API_KEY`, and an `example-gateway` binding in the client `config.toml` |

The three `research-and-write` examples are the same two-step Workflow (a
researcher with a `web_search` tool, then a writer) with each provider's
Prompts. The researcher's `tool_names` lists `web_search`, so the script
registers that tool with `@tool` before it loads the Workflow; loading fails
with a "runtime-local tool `web_search` was not found" error otherwise.

`gateway-demo` sends its first step through an external gateway with
`llm_route: ext_gateway` and its second step to OpenAI directly. Declare the
binding the step names before running it:

```toml
[workflow.external_gateway_bindings.example-gateway]
protocol = "openai_chat"
origin = "https://gateway.example.com"
secret_headers = { authorization = { source = "env", name = "GATEWAY_AUTHORIZATION" } }
```

These examples call live providers and are not part of `check:examples`.
