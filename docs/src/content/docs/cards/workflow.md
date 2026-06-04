---
title: Workflow
description: Generated reference for the Wyrd Workflow card spec.
---

# Workflow

Describe a coordinated sequence of operators, tools, agents, or services.

<dl class="wyrd-defs"><dt data-kind="workflow">Workflow</dt><dd>Describe a coordinated sequence of operators, tools, agents, or services.</dd><dt>Required</dt><dd>none required</dd><dt>Optional</dt><dd>7 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/workflow_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `description` | `string \| null` | no |
| `details` | `object` | no |
| `governance` | `object` | no |
| `inputs` | `object` | no |
| `observation_hooks` | `object` | no |
| `outputs` | `object` | no |
| `steps` | `array` | no |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a WorkflowCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/workflow_spec.json</code> is the current source of truth.</aside>

## Lifecycle

A `WorkflowCard` is the declarative spec; the live workflow engine lives in
`skald-workflow`. A Wyrd consumer turns a `WorkflowCard` into a live
`Workflow` by unwrapping the inner `WorkflowDef` and calling
`Workflow::from_def(def, providers, tools, observer)`. The engine binds every
agent, validates the task graph, and produces a runnable workflow.

`Workflow::run` returns a `WorkflowRun` with per-task outcomes, events, and the
terminal task id. `Workflow::execute_task` runs one task at a time for callers
that own their own loop.

See [agent runtime](/concepts/agent-runtime/) for handoff and observability.

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Workflow fits in the seven primitives.
- [Agent runtime](/concepts/agent-runtime/) — how WorkflowCards lower into live Skald workflows.
- [Card reference index](/cards/) — every kind in one place.
