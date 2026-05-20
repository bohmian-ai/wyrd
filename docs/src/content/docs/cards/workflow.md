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

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for WorkflowCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Workflow fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
