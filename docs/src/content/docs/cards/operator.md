---
title: Operator
description: Generated reference for the Wyrd Operator card spec.
---

# Operator

Describe an operator that performs bounded work inside a workflow or service.

<dl class="wyrd-defs"><dt data-kind="operator">Operator</dt><dd>Describe an operator that performs bounded work inside a workflow or service.</dd><dt>Required</dt><dd><code>adapter</code></dd><dt>Optional</dt><dd>4 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/operator_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `adapter` | `object` | yes |
| `budget` | `object` | no |
| `inputs` | `array` | no |
| `post_invoke` | `array` | no |
| `pre_invoke` | `array` | no |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a OperatorCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/operator_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for OperatorCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Operator fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
