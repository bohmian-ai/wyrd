---
title: Service
description: Generated reference for the Wyrd Service card spec.
---

# Service

Describe a deployable service and the runtime rules Wyrd can lock.

<dl class="wyrd-defs"><dt data-kind="service">Service</dt><dd>Describe a deployable service and the runtime rules Wyrd can lock.</dd><dt>Required</dt><dd>none required</dd><dt>Optional</dt><dd>11 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/service_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `components` | `array` | no |
| `content_hash` | `string \| null` | no |
| `credential_refs` | `array` | no |
| `deployment` | `object` | no |
| `description` | `string \| null` | no |
| `entry_point` | `string \| null` | no |
| `lock_hash` | `string \| null` | no |
| `metadata` | `object` | no |
| `runtime` | `object` | no |
| `service_config` | `object` | no |
| `service_type` | `string \| null` | no |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a ServiceCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/service_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for ServiceCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Service fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
