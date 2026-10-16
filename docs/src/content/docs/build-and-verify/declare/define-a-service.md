---
title: Define a Service
description: Declare a Service as a folder of Card files, with exact component versions and a table for the rows it records, then validate it locally.
pillar: wyrd
section: Build and verify
topic: Declare
order: 1
draft: true
status: runnable
---

A Service Card is the root of your declaration. It names the components your service is built from, each pinned to an exact Card version, and the tables it records rows into. You write it in YAML whatever language the service is written in, and `wyrd plan` checks it without a server.

This page builds the `support-desk` Service from the [quickstart](/get-started/quickstart/). The finished files are in `examples/support-desk-app/service/`.

## 1. Write the components

Each component is its own Card file with an exact `metadata.version`. The support desk has an Agent and the Prompt it runs. The Agent points at the Prompt by relative path:

<Tabs>
<Tab label="support-agent.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-agent.yaml" lang="yaml" title="support-agent.yaml" />

</Tab>
<Tab label="support-prompt.yaml">

<CodeFromFile file="examples/support-desk-app/service/support-prompt.yaml" lang="yaml" title="support-prompt.yaml" />

</Tab>
</Tabs>

`execution_route` says how the Agent reaches its model: `wyrd_gateway` sends it through the gateway deployment named `openai`, which [Set up the LLM gateway](/connect/set-up-the-llm-gateway/) creates. Use `{kind: native}` when your own code calls the model.

## 2. Write the Service

The Service gives each component an `alias`, the name your code uses at runtime, such as `state.run("agent")`:

```yaml title="support-desk.yaml"
apiVersion: wyrd/v1
kind: Service
metadata:
  name: support-desk
  version: 1.0.0
  space: default
spec:
  service_type: agent
  tables:
    - name: tickets
      schema:
        type: object
        properties:
          question: {type: string}
          answer: {type: string}
        required: [question, answer]
  components:
    - alias: agent
      ref: ./support-agent.yaml
    - alias: prompt
      ref: ./support-prompt.yaml
```

| Field | Holds |
| --- | --- |
| `components[].alias` | The runtime name of the component, unique within the Service |
| `components[].ref` | A path to the component's Card file, or a registered Card such as `{kind: Model, name: churn-classifier, version: 1.4.2}` |
| `tables[].name` | A table to create as `vala.datasets.<name>` when the Service registers |
| `tables[].schema` | A JSON Schema object; the order of `properties` is the column order |
| `service_type` | A free-form label, such as `agent` or `api` |

A path is relative to the file that contains it. Every reference names one exact version: Wyrd has no version ranges, so a registered Service always records exactly what it was built from.

Verifiers attach through `verified_by`, which [Bind a Verifier](/build-and-verify/declare/bind-a-verifier/) adds next.

## 3. Check it locally

```bash
wyrd plan support-desk.yaml
```

`plan` loads the Service and every Card it references, validates them, and contacts no server. It prints `plan valid: 3 card(s)`, then one line per Card, such as `Service default/support-desk@1.0.0`. Fix anything it reports before you register.

## Set the space once

`space` groups related Cards, such as a team's. Instead of writing it in every file, put a `wyrd.toml` next to your Cards or in any parent directory:

```toml
[defaults]
space = "support"

[kind.Verifier]
space = "support-verifiers"
```

A Card's own `metadata.space` wins, then the `[kind.<Kind>]` value, then `[defaults]`, then `default`. `wyrd.toml` can also set `labels` and `annotations`, but never `name` or `version`.

## If it fails

| You see | Cause | Fix |
| --- | --- | --- |
| `WYRD_LOADER_400_INVALID_ENVELOPE` | A file is missing `apiVersion` or `kind`, a path points at no Card, a referenced Card has no exact `version`, or two Cards reference each other. `details.diagnostics` names each file | Fix the file it names |
| `WYRD_LOADER_400_PATH_ESCAPE` | A `ref` path leaves the declaration's directory tree | Move the Card inside it, or reference the registered Card instead |

## Next

[Bind a Verifier](/build-and-verify/declare/bind-a-verifier/) to say how the Service is checked.
