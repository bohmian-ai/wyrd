---
title: Data
description: Generated reference for the Wyrd Data card spec.
---

# Data

Describe a dataset, feature table, document set, or other data dependency.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/data_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `artifact_refs` | `array` | no |
| `audit_ref` | `object` | no |
| `data_type` | `string | null` | no |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `experiment_ref` | `object` | no |
| `schema_ref` | `object` | no |
| `source_uri` | `string | null` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
