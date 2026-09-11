---
id: TASK-012
title: Drift Card and Observe workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-004, REQ-015, REQ-016, REQ-017, REQ-018, REQ-080, REQ-081, REQ-084, REQ-085, REQ-088, REQ-089, REQ-090, REQ-115, REQ-127, REQ-128, REQ-131, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-016, INV-019, INV-021, INV-022, AC-001, AC-003, AC-008, AC-010, AC-011]
depends_on: [TASK-005, TASK-006]
parent_task:
remediates: []
---

# Outcome and value

Let data scientists move cleanly from an authored Drift declaration to the
canonical calculated-result investigation without confusing configuration,
raw observations, calculated reports, or alerts.

# Owner and write set

- Own `src/lib/features/cards/workspaces/drift/**`,
  `src/lib/features/observe/drift/**`, the `/observe/drift` route, typed mock
  projections, tests, and the Drift workspace registration module.
- Card view shows method/profile, signal/features or metric, baseline/Eval/Source,
  thresholds, publishers, and Trigger/Operator relationships separately from a
  contextual result summary and link.
- Observe view supports URL-backed Drift/subject/Service/Run/principal/method/
  signal/feature/verdict/time filters, definition context, calculated history,
  baseline/threshold overlays, sample quality, feature verdicts, and alerts.

# Locked decisions and non-goals

- Calculated reports are explicit server projections; raw observations cannot
  satisfy their type or UI. Card declarations never own result truth.
- Methods remain filters, not routes. Alerts remain contextual records, not a
  Card or `/observe/alerts` product.
- No drift calculation, inference, persistence, alert configuration, or
  method-specific route.

# Locked visual implementation authority

- Implement `C-11-light` and `C-11-dark` from
  [`cards.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards.svg)
  for Drift declaration plus contextual results, and `O-10-light` and
  `O-10-dark` from
  [`observe.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/observe.svg)
  for canonical calculated-result exploration. Their exact route, filters,
  fixture semantics, thresholds, material states, and cross-links are locked
  by the [C-11/O-10 ledger](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md#material-states-index).
- Implement `M-07-light` and `M-07-dark` from
  [`mobile.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/mobile.svg),
  retaining all visible filter chips, pass/fail feature rows, threshold breach,
  no-report gap, calculated/raw distinction, and internally scrolling dense rows.
- Preserve the accepted Definition-versus-Results hierarchy and canonical
  Observe ownership. Do not merge declaration and observation into a generic
  metric dashboard. Completion evidence must compare `C-11`, `O-10`, and
  `M-07` in both themes at their declared viewports.

# Ordered test scenarios

1. Card declaration renders exact method/signal/baseline/threshold/publisher
   wiring and keeps projected results visually separate.
2. Card-to-Observe navigation preserves exact Drift Card, subject, Service
   version, feature, and time scope.
3. Observe restores filters and renders calculated history, overlays, sample
   quality, per-feature verdicts, alerts, and correlation links.
4. Raw-observation, no-data, unauthorized, partial, and error fixtures cannot
   appear as a calculated or healthy report.
5. Trigger/Operator links and narrow/both-theme layouts retain all distinctions.

# Red-Green-Refactor

Prove the raw-versus-calculated type boundary first, then Card declaration and
Observe investigation. Reuse Observe filters and chart framing only.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/drift/DriftWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/drift/DriftJourney.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/drift/report-boundary.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record declaration/result and raw/calculated matrices plus C-11/O-10/M-07
comparisons. Stop before browser calculation or a durable query contract is
invented.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
