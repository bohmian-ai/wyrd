---
id: TASK-009
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 1
requirements: [REQ-023, REQ-024, REQ-050, REQ-051, REQ-052]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-008]
acceptance: [AC-002, AC-006, AC-007, AC-008]
depends_on: [TASK-003, TASK-004, TASK-007, TASK-008]
parent_task: null
remediates: []
---

# TASK-009 — Canonical Home, Observe, Card, and Service summaries

## Outcome and Value

Home attention, Observe overview, Card evidence, and Service operational views agree with the accepted Card, signal, and generic verification detail surfaces for the same retained data.

## Source-Backed Reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Home/Observe | current Home and Observe overview routes; pinned `home-live.ts::liveHome`, `observe-live.ts::liveOverview` | current Observe overview test; pinned browser Home/overview cases | live projections use obsolete result columns | Reuse layout/orchestration, replace meanings with TASK-007/008 models | None |
| Card evidence | pinned `CardEvidence.svelte`, `card-live.ts::cardEvidence` | TASK-004/005 Card consumers | stale result SQL | Reuse current exact Card/result selectors | None |
| Service | current `ServiceWorkspace.svelte`, `service-state.ts`; pinned `service-live.ts::{declaredBindings,liveService}` | current Service tests; pinned browser Service case | mock operational projection and stale result columns | Preserve existing workspace, feed canonical signal/result models | None |
| Links | current filter/Service URL helpers | tests `withScope...`, `observeHref...` | cross-surface selectors can drift | Extend existing typed URL owners | None |

## Owners, Scope, Consumers, and Prohibited Changes

- Owners: existing Home/Observe/Service/Card evidence presentation and server loads. Prohibited: independent health formulas, lifecycle-as-health, name-based Service attribution, old verification vocabulary, or URL selectors as authority.

## Approach

1. Compose accepted Card, telemetry, and verification projections rather than porting old SQL verbatim.
2. Adapt pinned Home/Card/Service presentation to current identities.
3. Make links consume or explicitly omit each selector.
4. Prove one retained evidence set across all surfaces and obtain visual acceptance.

## Ordered Implementation Scenarios

### Scenario 1 — Canonical summary agreement

**Behavior.** Counts, verdicts, status, correlation, sparse/partial/denied/unavailable states agree with their detail pages; Service health is earned from evidence.

**RED.** Extend existing Service test `healthy is earned from signal data, never inferred`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/service/ServiceWorkspace.test.ts -t 'healthy is earned from signal data, never inferred'`.

**GREEN.** Feed existing summary owners from TASK-003/007/008 projections; rerun related current Service and Observe overview tests.

**REFACTOR.** Delete summary-specific old result interpretations.

### Scenario 2 — Real cross-surface continuity

**Behavior.** One real dataset agrees across Home, Observe overview, Card evidence, Service, and destinations while links preserve only applicable context.

**RED.** Restore/adapt pinned cases `live UI initial Home`, `live UI Observe overview`, and `live UI Service workspace`; exact command for each is `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='<case>' mise run test:identity:journey`.

**GREEN.** Complete live loads/links and rerun all three cases.

**REFACTOR.** Keep one display meaning per fact.

## Acceptance Criteria

- AC-002/006/007/008 pass for these surfaces; desktop/mobile evidence is accepted.

## Expected Write Set and Consumer Closure

- Home/Observe overview routes/components, Card evidence, existing Service workspace/projection/fixtures/tests, URL helpers and journey cases. Close TASK-003/007/008 consumers and permission/partial states.

## Verification and Evidence

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/observe/core/ObserveJourney.test.ts src/lib/features/cards/workspaces/service/ServiceWorkspace.test.ts src/lib/features/cards/workspaces/service/service-state.test.ts
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Service workspace' mise run test:identity:journey
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
```

## Material Stop Conditions

- Agreement requires unsupported totals or inferred Service ownership; canonical scopes materially disagree; or visual acceptance is withheld.

## Authority Links

- [Approved specification](../spec.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)

