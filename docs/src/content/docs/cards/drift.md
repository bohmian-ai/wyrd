---
title: Drift
description: Generated reference for the Wyrd Drift card spec.
---

# Drift

Describe the drift contract Wyrd uses to watch behavior over time.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/drift_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `baseline_ref` | `object` | no |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `features` | `array` | no |
| `method` | `object` | yes |
| `profile` | `object` | no |
| `target_refs` | `array` | no |
| `thresholds` | `object` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
