---
title: Operator
description: Generated reference for the Wyrd Operator card spec.
---

# Operator

Describe an operator that performs bounded work inside a workflow or service.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/operator_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `adapter` | `object` | yes |
| `budget` | `object` | no |
| `inputs` | `array` | no |
| `post_invoke` | `array` | no |
| `pre_invoke` | `array` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
