---
title: Drift
description: Generated reference for the Wyrd Drift card spec.
---

# Drift

Describe the drift contract Wyrd uses to watch behavior over time.

<dl class="wyrd-defs"><dt data-kind="drift">Drift</dt><dd>Describe the drift contract Wyrd uses to watch behavior over time.</dd><dt>Required</dt><dd><code>condition</code>, <code>method</code>, <code>signal</code>, <code>subject_ref</code></dd><dt>Optional</dt><dd>3 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/drift_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `condition` | `object` | yes |
| `description` | `string \| null` | no |
| `details` | `object` | no |
| `method` | `object` | yes |
| `profile` | `object` | no |
| `signal` | `object` | yes |
| `subject_ref` | `object` | yes |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a DriftCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/drift_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for DriftCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Drift fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
