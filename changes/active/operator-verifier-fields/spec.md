---
id: SPEC-operator-verifier-fields
revision: 1
status: approved
approved_at: 2026-09-24
depends_on: SPEC-verified-change-contract (revision 36)
---

# Operator verifier fields

## Objective and user value

A failed-verification alert today can only quote a server-written sentence
(`summary`, e.g. "Drift verdict Drift: 3 of 20 features drifted."). Authors of
Slack, PagerDuty, and HTTP Operators cannot place the numbers themselves, so
they cannot write "3/20 features drifted" in a Slack header, put a pass rate in
a PagerDuty summary, or send the counts as separate JSON values to their own
service.

This change adds a small, typed, count-only set of fields per Verifier
implementation to the Operator failure context, so every Operator template can
reference them. It keeps the safety posture of the verified change contract:
alerts still carry aggregates only, never result detail.

## Current behavior (baseline)

- `REQ-138` of `SPEC-verified-change-contract` fixes one closed failure context
  with 11 fields shared by every Verifier kind. Registration rejects unknown
  template fields.
- The context is frozen as JSON on each dispatch row when a failed
  binding-created run settles, and every retry renders that frozen copy.
- Only `failed` verdicts create dispatches. A failed Drift run always has a
  scored report. A failed Eval run always has a task rollup.

## Requirements

### REQ-001 — Kind-specific count fields

The failure context gains exactly one kind-specific block, which matches the
failed run's Verifier implementation:

| Implementation | Template field | Meaning |
|---|---|---|
| `drift` | `drift.drifted_features` | Features whose own verdict is `drift` |
| `drift` | `drift.total_features` | Features scored in the report |
| `eval` | `eval.passed_tasks` | Tasks that passed |
| `eval` | `eval.failed_tasks` | Tasks that ran and did not pass |
| `eval` | `eval.total_tasks` | Tasks that ran |
| `eval` | `eval.pass_rate_percent` | `floor(100 * passed / total)`; `0` when `total` is `0` |

- All values are non-negative integers. They are rendered as base-10 digits
  with no separators.
- `pass_rate_percent` rounds down, so a partial pass never renders as `100`.
- The counts are derived from the same acknowledged result that produces the
  existing `summary`. The two can never disagree.
- The existing 11 fields and `summary` remain unchanged.

### REQ-002 — Persisted and wire shape

The frozen context and every schema that exposes it add one required field,
`verifier`, holding a closed union tagged by implementation:

```json
"verifier": { "implementation": "drift", "drifted_features": 3, "total_features": 20 }
"verifier": { "implementation": "eval", "passed_tasks": 7, "failed_tasks": 3, "total_tasks": 10, "pass_rate_percent": 70 }
```

- Unknown keys and unknown implementations are rejected.
- The verified change contract has not shipped, so no compatibility decoder,
  default, or migration for older frozen contexts is required.

### REQ-003 — Registration checks fields against the bound Verifier's kind

- Template fields are checked in every Operator template position the
  verified change contract allows: Slack `text`, PagerDuty `summary`, and the
  HTTP URL path/query, header values, and JSON body string values.
- A standalone Operator Card may reference any known field of either kind.
- When a binding attaches Operators, registration resolves the binding's exact
  Verifier version. It rejects the binding with a stable validation error if
  any attached Operator references a field of a different implementation.
  Examples: an Eval Verifier's `on_failure` using `drift.total_features`, or a
  Drift Verifier's `on_failure` using `eval.pass_rate_percent`.
  - The error names the binding field path and the offending template field.
  - This applies equally to inline and referenced Operators.
- Because bindings pin an exact Verifier version, the kind cannot change after
  registration.
- A template referencing an unknown field keeps today's rejection. The allowed
  field list in that error includes the kind-specific names.

### REQ-004 — Delivery behavior

- Rendering substitutes the frozen values. HTTP JSON body placeholders render
  as JSON strings, the same as every existing field.
- If a frozen context lacks a referenced field (unreachable after REQ-003),
  the attempt fails terminal `invalid_request`. It never sends a partially
  rendered message.

### REQ-005 — Surfaces and documentation

- The public failure-context schema and generated Rust, Python, and TypeScript
  types project the new `verifier` union.
- `architecture/wyrd-design.md` lists the template fields per kind.
- Operator authoring docs show one Drift and one Eval Slack template using the
  new fields.

## Invariants

- **INV-001:** The context remains bounded and aggregate-only. No feature
  names, feature rows, Eval task inputs or outputs, assertion detail, media, or
  secret material enter it (preserves verified change contract `REQ-138`).
- **INV-002:** One context contract serves Notify, HTTP, and any later
  Workflow invocation. No second payload shape is introduced.
- **INV-003:** Frozen-once semantics are unchanged. Every retry renders
  identical values.

## Non-goals

- Drifted feature names or top-N features. This is result detail and needs its
  own decision.
- Slack Block Kit, a default Wyrd message layout, or user-authored Block Kit
  JSON.
- Slack escaping of inserted values.
- A link to the run in the UI.
- Conditional or looping template syntax, or number formatting options.
- Fields for Verifier implementations other than Drift and Eval. A future
  implementation adds its own block in its own change.

## Expensive-to-reverse decisions

1. **Namespaced field names** (`drift.*`, `eval.*`) in a closed template
   grammar. These become part of every stored Operator Card.
2. **The persisted/wire `verifier` union** on the frozen context (REQ-002).
3. **Registration-time kind checking at the binding.** Mismatches are
   rejected up front and do not render as empty at delivery.

## Acceptance criteria

- **AC-001:** A Drift binding's failed run delivers a Slack message rendering
  `drift.drifted_features`/`drift.total_features`. The values equal the
  report's counts and agree with `summary`. Evidence: server delivery journey
  against a local Slack mock.
- **AC-002:** An Eval binding's failed run delivers an HTTP JSON body carrying
  all four `eval.*` values, including a partial-pass `pass_rate_percent` that
  rounds down. Evidence: server delivery journey.
- **AC-003:** Registration rejects an Eval binding whose inline or referenced
  Operator uses a `drift.*` field, and the reverse. The error is stable and
  names the field path. A standalone Operator Card using both kinds registers.
  Evidence: HTTP registration journey plus one SDK or CLI projection.
- **AC-004:** Retries render the identical frozen values. Evidence: existing
  retry journey extended with a kind field.
- **AC-005:** Generated schemas and SDK types regenerate cleanly with the
  `verifier` union. Evidence: `mise run codegen:check`, Python/TS typecheck.

## Open material decisions

None. Revision 1 was explicitly approved by the user on 2026-09-24.

## Revision history

- **Revision 1 (2026-09-24):** Initial draft. Proposes kind-specific count
  fields, a closed `verifier` union on the frozen context, and binding-time
  kind checking. Explicitly approved by the user on 2026-09-24.

## Authority links

- `changes/active/verified-change-contract/spec.md` — `REQ-138`, `REQ-140`,
  `REQ-141`, `REQ-150`
- `architecture/wyrd-design.md` — Operator section
- `crates/wyrd-spec/src/card/operator.rs` — `OperatorFailureContext`
