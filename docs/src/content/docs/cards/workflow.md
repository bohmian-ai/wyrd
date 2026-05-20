---
title: Workflow
description: Generated reference for the Wyrd Workflow card spec.
---

# Workflow

Describe a coordinated sequence of operators, tools, agents, or services.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/workflow_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `governance` | `object` | no |
| `inputs` | `object` | no |
| `observation_hooks` | `object` | no |
| `outputs` | `object` | no |
| `steps` | `array` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
