---
id: TASK-017
title: Query OLAP workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-070, REQ-081, REQ-083, REQ-092, REQ-095, REQ-100, REQ-101, REQ-127, REQ-128, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-021, INV-022, AC-001, AC-003, AC-010, AC-011]
depends_on: [TASK-003]
parent_task:
remediates: []
---

# Outcome and value

Deliver the final ordered product workspace: one raw, read-only Bifrost SQL
surface for catalog discovery, authorized query execution, and inspection of
results, execution details, and local session history.

# Owner and write set

- Own `/t/[tenantKey]/query`, `src/lib/features/query/**`, typed mock actions,
  fixtures, and focused tests.
- Provide searchable catalog/schema/table/column exploration, one unsaved SQL
  editor, run/cancel, visible state/timing, tabular results, and Results/Query
  Details/History panes.
- Accept supported context from Observe `Open in Query` while keeping all raw SQL
  authoring here.

# Locked decisions and non-goals

- The server action projects Oracle's authorized, single-`SELECT` contract and
  its function, timeout, row, byte, cancellation, and structured error limits.
- Browser renders returned schema/rows/status and never authorizes SQL or infers
  execution truth.
- No mutations, data loading, destination writes, admin, saved worksheets,
  editor tabs, charts, sharing, collaboration, profiles, or direct Oracle call.

# Ordered test scenarios

1. Catalog search selects schema/table/columns and respects tenant/Space scope.
2. Empty, rejected, running, successful, cancelled, and failed executions keep
   editor state and show accurate controls/recovery.
3. Results and Query Details expose query ID, status, elapsed time, rows, bytes,
   columns, returned values, and safe structured errors.
4. Session history restores a prior query without implying persistence.
5. Observe handoff stays editable and does not create a Logs SQL editor.
6. Dense results use controlled overflow; narrow catalog becomes a drawer with
   state intact in both themes.

# Red-Green-Refactor

Drive action states before result presentation, then catalog/history/handoff.
Reuse shared table/control states; do not create a notebook platform.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/query/QueryJourney.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/query/QueryResults.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/query/query-actions.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record execution-state, result-overflow, history, and Observe-handoff evidence
against `brand/renders/product/query.svg`. Stop before persistence, mutation,
charting, or client-side authorization is introduced.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
