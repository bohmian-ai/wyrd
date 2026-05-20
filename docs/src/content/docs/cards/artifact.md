---
title: Artifact
description: Generated reference for the Wyrd Artifact card spec.
---

# Artifact

Track an immutable artifact that belongs to a card, run, or service release.

<dl class="wyrd-defs"><dt>Artifact</dt><dd>Track an immutable artifact that belongs to a card, run, or service release.</dd><dt>Required</dt><dd><code>artifact_kind</code></dd><dt>Optional</dt><dd>8 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/artifact_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `artifact_kind` | `string` | yes |
| `artifact_uris` | `array` | no |
| `content_type` | `string \| null` | no |
| `external_uri` | `string \| null` | no |
| `framework_adapter` | `object` | no |
| `integrity` | `string \| null` | no |
| `metadata` | `object` | no |
| `schema_ref` | `object` | no |
| `size_bytes` | `integer \| null` | no |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a ArtifactCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/artifact_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for ArtifactCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Artifact fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
