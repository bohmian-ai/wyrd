---
id: TASK-016
title: Trigger Card workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-015, REQ-016, REQ-017, REQ-080, REQ-081, REQ-084, REQ-085, REQ-116, REQ-127, REQ-128, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-016, INV-021, INV-022, AC-003, AC-008, AC-010, AC-011]
depends_on: [TASK-006]
parent_task:
remediates: []
---

# Outcome and value

Explain exactly when and why a Trigger invokes one Operator, including optional
Eval/Drift source and subject scope, before showing its raw configuration or
separately projected firing history.

# Owner and write set

- Own only `src/lib/features/cards/workspaces/trigger/**`, its fixture projection,
  focused tests, and registration module.
- Lead with a plain-language wiring sentence; show schedule/timezone, optional
  observation source and subject filter, linked Operator, reachable publishing
  Service/components, source → Trigger → Operator flow, and projected history.

# Locked decisions and non-goals

- Scheduling belongs to Trigger, never Drift. Runtime firing history is a
  separate projection and cannot establish declaration state.
- No scheduler/editor, manual fire action, alert product, inferred reachability,
  timezone conversion that obscures the authored timezone, or Operator copy.

# Locked visual implementation authority

- Implement `C-13-light` and `C-13-dark` from
  [`cards.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards.svg)
  using the exact plain-language-first hierarchy, schedule/timezone, optional
  observation source and subject filter, linked Operator, source → Trigger →
  Operator flow, reachable publishing context, and separately labeled firing
  history in the [C-13 ledger entry](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md#c-13--trigger-card).
- Narrow behavior follows `R-CARD`: wiring and exact configuration remain
  readable before projected history, with direct links preserved. Do not
  substitute a scheduler form, generic relationship graph, or alert page.
- Completion evidence must compare both desktop themes and a 390 × 844 capture,
  including scheduled and observation-driven fixture states.

# Ordered test scenarios

1. Scheduled and observation-driven fixtures produce accurate plain-language
   wiring and exact configuration.
2. Optional source/subject filters are present only when declared and preserve
   exact Card references.
3. Operator and reachable Service/component links navigate directly.
4. Firing history remains labeled runtime projection with truthful empty/error
   states.
5. Schedule, flow, and links remain accessible at narrow width/both themes.

# Red-Green-Refactor

Drive declaration semantics before projected history. Reuse relationship and
table primitives; do not build a scheduler abstraction.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/trigger/TriggerWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record schedule/source/filter/link/history cases against C-13. Stop before
implementing scheduling or deriving runtime events in the UI.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
