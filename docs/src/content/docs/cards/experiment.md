---
title: Experiment
description: Generated reference for the Wyrd Experiment card spec.
---

# Experiment

Group work-in-progress runs and candidate changes under a single intent.

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/experiment_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `artifact_refs` | `array` | no |
| `best_run_ref` | `object` | no |
| `default_parameters` | `object` | no |
| `description` | `string | null` | no |
| `details` | `object` | no |
| `experiment_type` | `string | null` | no |
| `run_refs` | `array` | no |
| `summary_metrics` | `array` | no |
| `target_refs` | `array` | no |

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.
