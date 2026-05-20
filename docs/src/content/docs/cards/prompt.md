---
title: Prompt
description: Generated reference for the Wyrd Prompt card spec.
---

# Prompt

Version prompt content and the contract around its inputs and outputs.

<dl class="wyrd-defs"><dt data-kind="prompt">Prompt</dt><dd>Version prompt content and the contract around its inputs and outputs.</dd><dt>Required</dt><dd><code>template</code></dd><dt>Optional</dt><dd>15 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/prompt_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `audit_ref` | `object` | no |
| `content_hash` | `string \| null` | no |
| `description` | `string \| null` | no |
| `details` | `object` | no |
| `experiment_ref` | `object` | no |
| `input_schema` | `object` | no |
| `messages` | `array` | no |
| `model` | `string \| null` | no |
| `output_schema` | `object` | no |
| `parameters` | `object` | no |
| `provider` | `object` | no |
| `template` | `string` | yes |
| `template_format` | `string \| null` | no |
| `tool_refs` | `array` | no |
| `variable_specs` | `array` | no |
| `variables` | `array` | no |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a PromptCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/prompt_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for PromptCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Prompt fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
