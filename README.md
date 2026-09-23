![Wyrd logo](docs/src/assets/wyrd-mark.svg)

# Wyrd

**Open-source verification and assurance infrastructure for AI systems.**

Wyrd checks whether an AI service behaves as declared. Define the service and
what you expect, record what happens during real runs, and verify the results
against that declaration. Wyrd keeps each result tied to the evidence and
component versions behind it.

[Getting started](docs/src/content/docs/get-started/index.svx) ·
[Architecture](architecture/wyrd-design.md) ·
[Release tracker](https://github.com/orgs/bohmian-ai/projects/1) ·
[Contributing](CONTRIBUTING.md)

**Current status:** A source checkout supports local Card authoring and a
development server. [Try the current source](#try-the-current-source). The
verification example below shows the approved `v0.1.0` interface; its
end-to-end path is still being completed.

## Planned verification workflow: Pydantic AI service

Define the service once. This bundle contains one Agent and its Prompt, an Eval
Verifier, and the Slack Operator to run when verification fails.

```yaml
# service.yaml
apiVersion: wyrd/v1
kind: Service
metadata:
  name: support-service
  version: "1.0.0"
  space: default
spec:
  service_type: agent
  components:
    - alias: support_agent
      ref: ./agent.yaml
      verified_by:
        - verifier: ./verifier.yaml
          runs_on:
            kind: observations_ready
          on_failure:
            - kind: notify
              channel:
                kind: slack
                connection: primary-workspace
                channel_id: C0123456789
                text: Support Agent verification failed.
---
# agent.yaml
apiVersion: wyrd/v1
kind: Agent
metadata:
  name: support-agent
  version: "1.0.0"
  space: default
spec:
  prompt: ./prompt.yaml
  tool_names: []
  run_config:
    max_iterations: 1
---
# prompt.yaml
apiVersion: wyrd/v1
kind: Prompt
metadata:
  name: support-prompt
  version: "1.0.0"
  space: default
spec:
  provider: openai
  model: gpt-5.6-sol
  system: Answer questions using the published refund policy.
  messages:
    - "{{question}}"
---
# verifier.yaml
apiVersion: wyrd/v1
kind: Verifier
metadata:
  name: support-agent-eval
  version: "1.0.0"
  space: default
spec:
  implementation:
    kind: eval
    pass_gate:
      kind: all_pass
    tasks:
      explains_refunds:
        kind: assertion
        id: explains_refunds
        context_path: $.answer
        operator: contains_ignore_case
        expected: refund
```

Register the graph and hydrate its exact versions for the application:

```bash
wyrd apply ./support-service
wyrd get --kind Service --space default --name support-service \
  --version 1.0.0 --output-dir ./support-service-bundle
```

Load the Agent and Prompt into Pydantic AI, then record what happened:

```python
from pydantic import BaseModel
from pydantic_ai import Agent
from wyrd import WyrdState


class SupportExchange(BaseModel):
    question: str
    answer: str


state = WyrdState.from_path("./support-service-bundle")
state.start_bifrost()

declared_agent = state.agent("support_agent")
declared_prompt = declared_agent.prompt
instructions = "\n".join(
    message["content"] for message in declared_prompt.system_messages
)
agent = Agent(
    f"{declared_prompt.provider}:{declared_prompt.model}",
    instructions=instructions,
)

question = "What is the refund policy?"
result = agent.run_sync(question)

with state.run(card="support_agent") as run:
    run.observe.eval(
        SupportExchange(question=question, answer=result.output),
        session_id="support-session-123",
    )
state.shutdown()
```

Wyrd ties the observation to the exact Service, Agent, Prompt, and Verifier
versions. The Eval runs asynchronously after the observation is committed. A
failed verdict is retained as evidence and dispatches the configured Slack
Operator.

## Why Wyrd exists

An AI service can remain available and return valid responses while its quality,
safety, or intended behavior quietly degrades. Its behavior depends on changing
data, model versions, prompts, tools, providers, and runtime context, so testing
the application once is not enough.

Teams often respond with separate evaluation scripts, telemetry pipelines,
drift monitors, registries, policy checks, and audit systems. Each represents
the system differently, making verification inconsistent, difficult to
reproduce, and expensive to maintain.

Wyrd makes verification a shared infrastructure capability instead of something
every team rebuilds.

## How Wyrd verifies AI systems

```text
Declare the system and its expected behavior
        ↓
Observe real executions
        ↓
Evaluate behavior with Verifiers
        ↓
Record evidence and lineage
        ↓
Enforce policy or trigger action
```

- **Cards** identify the exact data, models, prompts, agents, workflows, and
  services being verified.
- **Verifiers** define reusable Drift and Agent Eval checks.
- **Runs and Observations** record what happened during execution.
- **Lineage** connects verification evidence to exact component versions.
- **Bifrost** stores and queries the evidence produced by verification.
- **Policy and Audit** govern consequential decisions and record
  accountability.
- **Operators** respond when verification fails.

## How the platform fits together

- **Wyrd** is the control plane for Cards, registry, lineage, identity, policy,
  and audit.
- **Vala** owns observations, Drift and Agent Eval execution, and the Bifrost
  warehouse.
- **Skald** owns LLM providers, prompts, tools, agents, and workflows.

## Try the current source

Public packages and container images will be published with `v0.1.0`. Until
then, work from a repository checkout. You need Git, Python 3.10 or newer, and
[`mise`](https://mise.jdx.dev/).

```bash
git clone https://github.com/bohmian-ai/wyrd.git
cd wyrd
mise install
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

This credential-free example creates a `DataCard` from a pandas DataFrame,
saves it locally, and loads it again. It demonstrates Card authoring; it does
not start the server or run continuous verification.

To run the current server locally:

```bash
mkdir -p .wyrd-dev/storage
export WYRD_STORAGE_URL="file://$PWD/.wyrd-dev/storage"
export WYRD_PUBLIC_BASE_URL="http://localhost:8080"
mise run dev:backend
```

The development server uses embedded Postgres, local object storage, and an
ephemeral signing key. See
[Self-hosting](docs/src/content/docs/self-hosting/index.svx) for health checks,
initialization, and production configuration.

## Release status

> **Pre-release:** Wyrd `v0.1.0` is targeted for September 25, 2026. It will
> be the first public release. Wyrd will remain pre-1.0, so APIs and persisted
> schemas may change between minor versions; `v0.1.0` will not include upgrade
> migrations.

Today, a source checkout supports local Card authoring and a development
server. `v0.1.0` adds the complete self-hosted verification path, public Rust,
Python, and TypeScript SDKs, and Linux Docker images. Windows is not part of
this release.

Follow release progress in the repository's releases and documentation.

## Documentation

- [Get started](docs/src/content/docs/get-started/index.svx)
- [Wyrd architecture](architecture/wyrd-design.md)
- [Bifrost architecture](architecture/bifrost-design.md)
- [Self-hosting](docs/src/content/docs/self-hosting/index.svx)
- [Security policy](SECURITY.md)

## Contributing

Issues and pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md)
before starting a substantial change so maintainers can coordinate overlapping
work.

Report vulnerabilities privately through [SECURITY.md](SECURITY.md), not
through a public issue.

## License

Wyrd is licensed under the [Apache License 2.0](LICENSE.md).
