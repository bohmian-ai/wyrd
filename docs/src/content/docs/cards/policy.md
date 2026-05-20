---
title: Policy
description: Generated reference for the Wyrd Policy card spec.
---

# Policy

Capture rules that decide whether a card, run, or action is allowed.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/policy_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `enforcement` | `string | null` | no |
| `rules` | `array` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
