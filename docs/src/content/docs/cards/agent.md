---
title: Agent
description: Generated reference for the Wyrd Agent card spec.
---

# Agent

Declare an agent that can be evaluated, governed, installed, and invoked through Wyrd.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/agent_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `capabilities` | `array` | no |
| `default_input_modes` | `array` | no |
| `default_output_modes` | `array` | no |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `documentation_url` | `string | null` | no |
| `governance` | `object` | no |
| `icon_url` | `string | null` | no |
| `interfaces` | `array` | no |
| `max_iterations` | `integer | null` | no |
| `memory_ref` | `object` | no |
| `model` | `string | null` | no |
| `observation_hooks` | `object` | no |
| `prompt_ref` | `object` | no |
| `prompt_refs` | `array` | no |
| `protocol_profiles` | `array` | no |
| `provider` | `object` | no |
| `provider_metadata` | `object` | no |
| `security_requirements` | `array` | no |
| `security_schemes` | `array` | no |
| `signatures` | `object` | no |
| `skill_refs` | `array` | no |
| `subagent_refs` | `array` | no |
| `tool_refs` | `array` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
