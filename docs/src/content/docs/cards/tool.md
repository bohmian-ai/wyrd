---
title: Tool
description: Generated reference for the Wyrd Tool card spec.
---

# Tool

Declare an executable tool and the constraints around its use.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/tool_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `allowed_tools` | `array` | no |
| `api_config` | `object` | no |
| `args_schema` | `object` | yes |
| `credential_refs` | `array` | no |
| `description` | `string` | yes |
| `details` | `object` | no |
| `hook_events` | `array` | no |
| `hook_matcher` | `object` | no |
| `mcp_server_name` | `string | null` | no |
| `name` | `string` | yes |
| `output_schema` | `object` | no |
| `requires_approval` | `boolean` | no |
| `script_config` | `object` | no |
| `tool_type` | `string` | yes |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
