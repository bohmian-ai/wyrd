---
id: TASK-007
title: Service Card operational workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-015, REQ-016, REQ-017, REQ-018, REQ-080, REQ-081, REQ-084, REQ-085, REQ-112, REQ-113, REQ-123, REQ-124, REQ-125, REQ-126, REQ-127, REQ-128, REQ-131, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-016, INV-019, INV-020, INV-021, INV-022, AC-003, AC-008, AC-010, AC-011]
depends_on: [TASK-006]
parent_task:
remediates: []
---

# Outcome and value

Make one exact Service Card version a concise operational plane of glass: a
mini-dashboard that answers current behavior and the next investigation before
exposing composition or raw definition.

# Owner and write set

- Own only `src/lib/features/cards/workspaces/service/**`, its typed mock
  projection, tests, and registration module.
- Implement exactly Overview, Composition, and Definition as URL-restorable
  local views. Overview is default.
- Overview leads with one assessment, then request rate, error rate, latency,
  earned availability/custom metrics, publication-driven Drift/Eval trends, and
  restrained attention/activity/component context.
- Composition implements the accepted deterministic linked-Card graph and
  contextual inspection; Definition presents the declaration and closed raw
  Spec disclosure.

# Locked decisions and non-goals

- Card, deployment, operational, and freshness states remain separate. Every
  signal is scoped to selected Service version and visible time range.
- All signal links target canonical Observe routes and retain subject/time scope
  plus a return path. Missing, stale, partial, unauthorized, and failed data
  never appears healthy.
- No dashboard builder, arbitrary layout, browser-owned chart configuration,
  Alert Card/route, Service signal route tree, or graph-first Overview.

# Ordered test scenarios

1. Overview restores version/time state and shows one assessment plus dominant
   operational trends with units, sources, freshness, and thresholds.
2. Healthy, attention, stale/no-data, partial-authorization, backend-failure,
   historical-version, saved custom chart, and Add-chart states remain truthful.
3. Drift/Eval panels appear only for declared publication bindings, identify the
   publishing subject, and preserve scope into Observe.
4. Composition preserves direction, aliases, Card identities/versions/status,
   selection, and direct links without claiming runtime execution.
5. Definition preserves human-readable declaration and safe raw disclosure.
6. Narrow and both-theme views remain scannable and non-color-dependent.

# Red-Green-Refactor

Drive Overview states first, then Composition, then Definition. Reuse the core
chart frame; keep Service aggregation and copy local to this workspace.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/service/ServiceWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/service/service-state.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record all Service state variants, canonical links, and paired comparisons to
`brand/renders/product/cards/service/`. Stop if a browser calculation is needed
to establish health or if custom-chart persistence lacks a server contract.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
