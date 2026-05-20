---
title: Skill
description: Generated reference for the Wyrd Skill card spec.
---

# Skill

Declare a reusable skill that an agent can select with predictable inputs.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/skill_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `input_schema` | `object` | no |
| `output_schema` | `object` | no |
| `prompt_refs` | `array` | no |
| `tool_refs` | `array` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
