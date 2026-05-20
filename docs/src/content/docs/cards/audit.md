---
title: Audit
description: Generated reference for the Wyrd Audit card spec.
---

# Audit

Represent a reviewable event or decision that needs durable provenance.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/audit_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `evidence_refs` | `array` | no |
| `policy_refs` | `array` | no |
| `subject_refs` | `array` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
