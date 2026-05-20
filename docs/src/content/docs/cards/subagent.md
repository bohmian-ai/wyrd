---
title: SubAgent
description: Generated reference for the Wyrd SubAgent card spec.
---

# SubAgent

Describe a narrower agent role that can be called by a parent agent.

<dl class="wyrd-defs"><dt data-kind="subagent">SubAgent</dt><dd>Describe a narrower agent role that can be called by a parent agent.</dd><dt>Required</dt><dd>none required</dd><dt>Optional</dt><dd>19 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/subagent_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `background` | `boolean` | no |
| `compatible_clis` | `array` | no |
| `description` | `string \| null` | no |
| `details` | `object` | no |
| `disallowed_tools` | `array` | no |
| `effort` | `string \| null` | no |
| `governance` | `object` | no |
| `isolation` | `string \| null` | no |
| `max_turns` | `integer \| null` | no |
| `mcp_servers` | `array` | no |
| `memory` | `object` | no |
| `model` | `string \| null` | no |
| `observation_hooks` | `object` | no |
| `permission_mode` | `string \| null` | no |
| `sandbox_mode` | `string \| null` | no |
| `skill_refs` | `array` | no |
| `system_prompt` | `string \| null` | no |
| `temperature` | `number \| null` | no |
| `tool_refs` | `array` | no |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a SubAgentCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/subagent_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for SubAgentCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where SubAgent fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
