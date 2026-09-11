---
id: TASK-005
title: Observe traces metrics and logs workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-004, REQ-071, REQ-072, REQ-073, REQ-074, REQ-076, REQ-077, REQ-078, REQ-079, REQ-081, REQ-084, REQ-085, REQ-091, REQ-092, REQ-095, REQ-100, REQ-101, REQ-127, REQ-128, REQ-131, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-012, INV-014, INV-021, INV-022, AC-001, AC-003, AC-010, AC-011]
depends_on: [TASK-003]
parent_task:
remediates: []
---

# Outcome and value

Deliver the second product workspace: a signal-first operational flow centered
on Traces, Metrics, and Logs, with compact overview and read-only dashboards,
shared correlation filters, and direct investigation paths.

# Owner and write set

- Own Observe overview, Logs, Metrics, Traces, trace detail, dashboard inventory
  and read-only detail routes plus `src/lib/features/observe/core/**`.
- Establish Observe local navigation and one URL-backed time/correlation filter
  contract that TASK-012 and TASK-013 can consume without editing this feature.
- Logs provide guided search, trend, records, structured detail, and `Open in
  Query`; Metrics provide discovery, labels, charts, and values; Traces provide
  trend plus results and an inspectable waterfall/service graph/span detail.
- Overview stays compact; dashboards are read-only and not a builder.

# Locked decisions and non-goals

- Canonical signal pages remain unfiltered homes. Service, Card, Run,
  experiment, principal, request, trace/span, status, time, and signal dimensions
  are removable/restorable filters, not route hierarchies.
- The BFF projects stored correlation and server results; the browser never
  fabricates joins or health.
- No Eval/Drift implementation here, alerting, scheduled search, dashboard edit,
  raw SQL editor, direct Bifrost call, or Service-owned observation route.

# Locked visual implementation authority

- Implement `O-01` through `O-07` from
  [`observe.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/observe.svg):
  overview, Logs, Metrics, Traces, trace detail, dashboard inventory, and
  dashboard detail. The [ledger entries](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md#observesvg)
  lock each route, URL scope, material state, link, selected-detail treatment,
  and responsive pattern.
- [`golden-O-04.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/golden-O-04.svg)
  is the immutable Gate A reference for operational Workbench composition:
  filters, trend, records, selected row, raised detail, density, and hierarchy.
  Apply that grammar across `O-01`–`O-07`; do not flatten the pages into a
  repeated table-and-rail template.
- Implement `M-05` from
  [`mobile.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/mobile.svg)
  for trace detail. Other Observe pages follow their exact `R-SIGNAL`, `R-LIST`,
  or `R-DETAIL` rules in the ledger: filters remain visible, dense content
  scrolls within its panel, and selected detail stacks after the dominant work.
- Completion evidence must compare `O-01`–`O-07` in both themes and `M-05` at
  390 × 844. It must demonstrate every alternate state named in the ledger at
  the same semantic page position rather than in a component-demo strip.

# Ordered test scenarios

1. Overview and local navigation restore shared URL scope and preserve it in
   cross-signal links.
2. Traces search common facets, show trend plus table, and retain return context
   through selected-span trace detail.
3. Metrics discover/filter a measure and render chart plus underlying values.
4. Logs search/inspect structured records and transfer equivalent context to
   Query without embedding SQL authoring.
5. Dashboard inventory/detail is read-only and reuses shared chart framing.
6. Loading, no-data, unauthorized, partial, and safe-error states never imply
   healthy data; narrow and both-theme views remain operable.

# Red-Green-Refactor

Drive Traces, then Metrics, then Logs, then overview/dashboard closure. Extract
only shared Observe filter/navigation behavior used by multiple signal pages.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/TraceDetail.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/FilterState.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record route/filter/link matrices and paired responsive comparisons. Stop before
building query authorization, dashboard configuration, correlation inference,
or a second component system inside Observe.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
