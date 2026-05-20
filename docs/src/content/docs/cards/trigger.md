---
title: Trigger
description: Generated reference for the Wyrd Trigger card spec.
---

# Trigger

Describe an event source that can start a workflow or service action.

<dl class="wyrd-defs"><dt data-kind="trigger">Trigger</dt><dd>Describe an event source that can start a workflow or service action.</dd><dt>Required</dt><dd><code>source</code>, <code>target</code></dd><dt>Optional</dt><dd>2 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/trigger_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `config` | `object` | no |
| `cooldown_seconds` | `integer \| null` | no |
| `source` | `object` | yes |
| `target` | `object` | yes |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a TriggerCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/trigger_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for TriggerCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Trigger fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
