---
title: Eval
description: Generated reference for the Wyrd Eval card spec.
---

# Eval

Define an evaluation suite, scoring rule, or acceptance check.

<dl class="wyrd-defs"><dt data-kind="eval">Eval</dt><dd>Define an evaluation suite, scoring rule, or acceptance check.</dd><dt>Required</dt><dd><code>tasks</code></dd><dt>Optional</dt><dd>6 additional spec fields — see table below.</dd></dl>

This page is generated from the checked-in JSON Schema. Edit the Rust spec, run the schema generator, then run `mise run docs:generate` to refresh this page.

## Source

- Schema: `crates/wyrd-spec/schemas/eval_spec.json`

## Fields

| Field | Type | Required |
| --- | --- | --- |
| `context_capture` | `object` | no |
| `dataset` | `object` | no |
| `pass_gate` | `object` | no |
| `sampling` | `object` | no |
| `subject_ref` | `object` | no |
| `tasks` | `object` | yes |
| `workflow` | `object` | no |

## Shape

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Copy-pasteable YAML for a EvalCard lands with the Card write path in Phase 5a. The JSON Schema at <code>crates/wyrd-spec/schemas/eval_spec.json</code> is the current source of truth.</aside>

## Lifecycle

<aside class="wyrd-phase-gate"><span class="wyrd-phase-gate__badge">Phase 5a</span>Write, version, transition, and retire flows for EvalCards land in Phase 5a.</aside>

## Authoring notes

- Keep card names stable; downstream runs, policies, and audits refer to them by identity.
- Put operational rules in the spec instead of burying them in free-form notes.
- Prefer explicit references to other cards when a dependency matters at runtime.

## Related

- [Concepts overview](/concepts/) — where Eval fits in the seven primitives.
- [Card reference index](/cards/) — every kind in one place.
