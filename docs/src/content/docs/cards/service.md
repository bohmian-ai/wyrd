---
title: Service
description: Generated reference for the Wyrd Service card spec.
---

# Service

Describe a deployable service and the runtime rules Wyrd can lock.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/service_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `components` | `array` | no |
| `content_hash` | `string | null` | no |
| `credential_refs` | `array` | no |
| `deployment` | `object` | no |
| `description` | `string | null` | no |
| `entry_point` | `string | null` | no |
| `lock_hash` | `string | null` | no |
| `metadata` | `object` | no |
| `runtime` | `object` | no |
| `service_config` | `object` | no |
| `service_type` | `string | null` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
