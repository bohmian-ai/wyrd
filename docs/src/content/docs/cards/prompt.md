---
title: Prompt
description: Generated reference for the Wyrd Prompt card spec.
---

# Prompt

Version prompt content and the contract around its inputs and outputs.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/prompt_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `audit_ref` | `object` | no |
| `content_hash` | `string | null` | no |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `experiment_ref` | `object` | no |
| `input_schema` | `object` | no |
| `messages` | `array` | no |
| `model` | `string | null` | no |
| `output_schema` | `object` | no |
| `parameters` | `object` | no |
| `provider` | `object` | no |
| `template` | `string` | yes |
| `template_format` | `string | null` | no |
| `tool_refs` | `array` | no |
| `variable_specs` | `array` | no |
| `variables` | `array` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
