---
id: TASK-007
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 1
requirements: [REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-030, REQ-031, REQ-032, REQ-033, REQ-052]
invariants: [INV-002, INV-003, INV-004, INV-007, INV-008]
acceptance: [AC-003, AC-007, AC-008]
depends_on: [TASK-001, TASK-002]
parent_task: null
remediates: []
---

# TASK-007 — Live Observe traces, logs, and metrics

## Outcome and Value

Observe provides one shared URL-backed investigation model and live trace, log, and metric pages. Each signal is visually accepted before the next scenario proceeds.

## Source-Backed Reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Shared controls | current `filter-state.ts::{readScope,scopeQuery,withScope}`, Observe route/components; pinned `ObserveToolbar`, `signal-reads.svelte.ts`, `zoom.ts`, `filter-expression.ts` | current `FilterState.test.ts`, `ObserveJourney.test.ts`; pinned filter/time cases | main uses synchronous fixtures and lacks cancellation/filter grammar | Extend current state with pinned proven behavior | No new framework |
| Traces | pinned `observe-live.ts::{traceQueries,liveTraces,liveTrace}` | current `TraceDetail.test.ts`; pinned trace projection/browser cases | no live read | Adapt to current `vala.traces.spans` schema through TASK-001 query | None |
| Logs | pinned `{logQueries,liveLogs}` | current log journey tests; pinned live logs | no live read | Adapt to current `vala.logs.records` | None |
| Metrics | pinned `{metricCatalogSql,metricSeriesSql,metricPointsSql,metricMeasure,counterRate,histogramQuantiles,liveMetrics}` | current metric journey; pinned metric projection/calculation/browser cases | no live read | Reuse schema-valid query/math only | None |

## Owners, Scope, Consumers, and Prohibited Changes

- Owners: existing Observe routes/components/state plus pinned server projections. Prohibited: raw URL SQL, invented correlation/log IDs/zeroes/percentiles, incompatible series merging, unbounded reads, browser credentials, stale response overwrite.

## Approach

1. Establish shared time/filter/zoom/refresh/cancel URL state and authenticated incremental reads only where page navigation is insufficient.
2. Port traces, then logs, then metrics against current table schemas.
3. After each signal, run exact component and live cases and pause for visual acceptance.

## Ordered Implementation Scenarios

### Scenario 1 — Shared controls and traces

**Behavior.** Newer investigations supersede old reads; bounded trace list/detail preserves hierarchy, events, links, status, permitted attributes, paging and return context.

**RED.** Restore pinned tests `live UI trace list projection` and `live UI trace detail projection`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts -t 'live UI trace list projection'` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/TraceDetail.test.ts -t 'live UI trace detail projection'`.

**GREEN.** Port current-schema trace projections and pinned control/cancellation behavior; run `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI trace list' mise run test:identity:journey` and the same with `live UI trace detail`, `live UI filter expression`, and `live UI time and refresh`.

**REFACTOR.** Keep one investigation state owner; obtain trace visual acceptance.

### Scenario 2 — Logs

**Behavior.** Bounded deterministic records/detail preserve equal rows, nullable correlation, protected body denial, paging, facets, and trace links.

**RED.** Restore pinned test `live UI logs projection`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts -t 'live UI logs projection'`.

**GREEN.** Adapt pinned log reads and run `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI logs' mise run test:identity:journey`.

**REFACTOR.** Remove mock-only record identities; obtain log visual acceptance.

### Scenario 3 — Metrics

**Behavior.** Counter/gauge/distribution series preserve type, unit, temporality, grouping, gaps, freshness, supported rate/quantile math, and zoom.

**RED.** Restore pinned tests `live UI metrics projection` and `live UI metric calculations`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts -t 'live UI metrics projection'` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts -t 'live UI metric calculations'`.

**GREEN.** Adapt pinned metric reads/math and run `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI metrics' mise run test:identity:journey` plus `live UI trace duration and metric calculations`.

**REFACTOR.** Keep pure math synchronous; obtain metric visual acceptance.

## Acceptance Criteria

- AC-003/007/008 pass for all signals, including sparse/dense, denied, empty, unavailable, cancellation, and correlation states.

## Expected Write Set and Consumer Closure

- Existing Observe routes/components/state/styles/tests; pinned live projection/filter/read components; facade; fixtures; journey cases. Close Query links, Card/Run/trace context, auth, request disconnect, and overview consumers.

## Verification and Evidence

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/FilterState.test.ts src/lib/features/observe/core/ObserveJourney.test.ts src/lib/features/observe/core/TraceDetail.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
```

## Material Stop Conditions

- Current tables cannot preserve required identity/type/temporality; incremental reads would bypass auth; or a visual checkpoint is withheld.

## Authority Links

- [Approved specification](../spec.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Canvas reference](../references/canvas/canvas.json)

