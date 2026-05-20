---
title: Artifact
description: Generated reference for the Wyrd Artifact card spec.
---

# Artifact

Track an immutable artifact that belongs to a card, run, or service release.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/artifact_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `artifact_kind` | `string` | yes |
| `artifact_uris` | `array` | no |
| `content_type` | `string | null` | no |
| `external_uri` | `string | null` | no |
| `framework_adapter` | `object` | no |
| `integrity` | `string | null` | no |
| `metadata` | `object` | no |
| `schema_ref` | `object` | no |
| `size_bytes` | `integer | null` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
