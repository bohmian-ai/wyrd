---
title: Model
description: Generated reference for the Wyrd Model card spec.
---

# Model

Describe a model artifact, its interface, and the context needed to use it safely.

<dl class="wyrd-defs"><dt data-kind="model">Model</dt><dd>Describe a model artifact, its interface, and the context needed to use it safely.</dd><dt>Required</dt><dd><code>interface</code>, <code>signature</code>, <code>task_type</code></dd><dt>Optional</dt><dd>2 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/model_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `card_refs` | `array` | no |
| `interface` | `object` | yes |
| `sample_input` | `object` | no |
| `signature` | `object` | yes |
| `task_type` | `object` | yes |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a ModelCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/model_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for ModelCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Model fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
