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
