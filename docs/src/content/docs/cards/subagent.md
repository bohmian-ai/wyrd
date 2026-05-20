---
title: SubAgent
description: Generated reference for the Wyrd SubAgent card spec.
---

# SubAgent

Describe a narrower agent role that can be called by a parent agent.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/subagent_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `background` | `boolean` | no |
| `compatible_clis` | `array` | no |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `disallowed_tools` | `array` | no |
| `effort` | `string | null` | no |
| `governance` | `object` | no |
| `isolation` | `string | null` | no |
| `max_turns` | `integer | null` | no |
| `mcp_servers` | `array` | no |
| `memory` | `object` | no |
| `model` | `string | null` | no |
| `observation_hooks` | `object` | no |
| `permission_mode` | `string | null` | no |
| `sandbox_mode` | `string | null` | no |
| `skill_refs` | `array` | no |
| `system_prompt` | `string | null` | no |
| `temperature` | `number | null` | no |
| `tool_refs` | `array` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
