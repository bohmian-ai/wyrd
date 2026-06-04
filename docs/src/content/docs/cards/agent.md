---
title: Agent
description: Generated reference for the Wyrd Agent card spec.
---

# Agent

Declare an agent that Wyrd can describe, govern, evaluate, install references for, and observe. Execution stays in the user or framework runtime. Wyrd does not host user agent loops or execute user agent code.

<dl class="wyrd-defs"><dt data-kind="agent">Agent</dt><dd>Declare an agent that Wyrd can describe, govern, evaluate, install references for, and observe. Execution stays in the user or framework runtime. Wyrd does not host user agent loops or execute user agent code.</dd><dt>Required</dt><dd>none required</dd><dt>Optional</dt><dd>24 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/agent_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `capabilities` | `array` | no |
| `default_input_modes` | `array` | no |
| `default_output_modes` | `array` | no |
| `description` | `string \| null` | no |
| `details` | `object` | no |
| `documentation_url` | `string \| null` | no |
| `governance` | `object` | no |
| `icon_url` | `string \| null` | no |
| `interfaces` | `array` | no |
| `max_iterations` | `integer \| null` | no |
| `memory_ref` | `object` | no |
| `model` | `string \| null` | no |
| `observation_hooks` | `object` | no |
| `prompt_ref` | `object` | no |
| `prompt_refs` | `array` | no |
| `protocol_profiles` | `array` | no |
| `provider` | `string \| null` | no |
| `provider_metadata` | `object` | no |
| `security_requirements` | `array` | no |
| `security_schemes` | `array` | no |
| `signatures` | `object` | no |
| `skill_refs` | `array` | no |
| `subagent_refs` | `array` | no |
| `tool_refs` | `array` | no |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a AgentCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/agent_spec.json</code> is the current source of truth.</aside>

## Lifecycle

An `AgentCard` is the declarative spec; the live agent runtime lives in
`skald-agent`. A Wyrd consumer turns an `AgentCard` into a live `Agent` by
unwrapping the inner `AgentDef`, binding a provider client from the
`skald-runtime` `ProviderRegistry`, resolving tools from the Wyrd-side tool
registry, and constructing the live agent via `Agent::from_def`.

See [agent runtime](/concepts/agent-runtime/) for the layering and the
`Observer` hook surface.

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Agent fits in the seven primitives.
- [Agent runtime](/concepts/agent-runtime/) — how AgentCards lower into live Skald agents.
- [Card reference index](/cards/) — every kind in one place.
