---
title: Policy
description: Generated reference for the Wyrd Policy card spec.
---

# Policy

Capture rules that decide whether a card, run, or action is allowed.

<dl class="wyrd-defs"><dt>Policy</dt><dd>Capture rules that decide whether a card, run, or action is allowed.</dd><dt>Required</dt><dd>none required</dd><dt>Optional</dt><dd>4 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/policy_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `description` | `string \| null` | no |
| `details` | `object` | no |
| `enforcement` | `string \| null` | no |
| `rules` | `array` | no |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a PolicyCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/policy_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for PolicyCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Policy fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
