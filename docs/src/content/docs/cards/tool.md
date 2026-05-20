---
title: Tool
description: Generated reference for the Wyrd Tool card spec.
---

# Tool

Declare an executable tool and the constraints around its use.

<dl class="wyrd-defs"><dt data-kind="tool">Tool</dt><dd>Declare an executable tool and the constraints around its use.</dd><dt>Required</dt><dd><code>args_schema</code>, <code>description</code>, <code>name</code>, <code>tool_type</code></dd><dt>Optional</dt><dd>10 additional spec fields — see table below.</dd></dl>

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
| `mcp_server_name` | `string \| null` | no |
| `name` | `string` | yes |
| `output_schema` | `object` | no |
| `requires_approval` | `boolean` | no |
| `script_config` | `object` | no |
| `tool_type` | `string` | yes |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a ToolCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/tool_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for ToolCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Tool fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
