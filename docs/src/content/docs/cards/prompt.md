---
title: Prompt
description: Generated reference for the Wyrd Prompt card spec.
---

# Prompt

Version prompt content and the contract around its inputs and outputs.

<dl class="wyrd-defs"><dt data-kind="prompt">Prompt</dt><dd>Version prompt content and the contract around its inputs and outputs.</dd><dt>Required</dt><dd><code>model</code>, <code>request</code></dd><dt>Optional</dt><dd>4 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/prompt_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `media_variables` | `array` | no |
| `model` | `string` | yes |
| `request` | `object` | yes |
| `response_type` | `object` | no |
| `variables` | `array` | no |
| `version` | `string \| null` | no |

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
