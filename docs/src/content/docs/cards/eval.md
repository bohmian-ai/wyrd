---
title: Eval
description: Generated reference for the Wyrd Eval card spec.
---

# Eval

Define an evaluation suite, scoring rule, or acceptance check.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/eval_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `assertions` | `array` | no |
| `dataset_refs` | `array` | no |
| `default_parameters` | `object` | no |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `eval_type` | `object` | yes |
| `governance` | `object` | no |
| `judge_refs` | `array` | no |
| `observation_hooks` | `object` | no |
| `pass_gates` | `array` | no |
| `profile` | `object` | no |
| `target_refs` | `array` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
