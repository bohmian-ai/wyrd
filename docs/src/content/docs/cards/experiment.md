---
title: Experiment
description: Generated reference for the Wyrd Experiment card spec.
---

# Experiment

Group work-in-progress runs and candidate changes under a single intent.

<dl class="wyrd-defs"><dt data-kind="experiment">Experiment</dt><dd>Group work-in-progress runs and candidate changes under a single intent.</dd><dt>Required</dt><dd>none required</dd><dt>Optional</dt><dd>9 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/experiment_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `best_run_ref` | `object` | no |
| `card_refs` | `array` | no |
| `default_parameters` | `object` | no |
| `description` | `string \| null` | no |
| `details` | `object` | no |
| `experiment_type` | `string \| null` | no |
| `run_refs` | `array` | no |
| `summary_metrics` | `array` | no |
| `target_refs` | `array` | no |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a ExperimentCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/experiment_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for ExperimentCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Experiment fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
