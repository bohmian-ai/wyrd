---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 1
requirements: [REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-020, REQ-052]
invariants: [INV-001, INV-002, INV-004, INV-005, INV-007, INV-008]
acceptance: [AC-002, AC-007, AC-008]
depends_on: [TASK-001, TASK-002]
parent_task: null
remediates: []
---

# TASK-003 — Live Card inventory

## Outcome and Value

`/t/{tenant}/cards` lists authorized current-main Cards with supported URL filters and opaque cursor paging; mock and live states remain explicit and visually reviewable.

## Source-Backed Reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Page/presentation | current cards `+page.server.ts::load`, `+page.svelte`, `CardsInventory.test.ts`, `wyrd.ts::WyrdClient::cards` | route load and row links | synchronous mock projection claims counts/sorting unavailable from server | Keep route/component; make live method async and truthful | None |
| Registry read | `cards/routes.rs::{list_cards_http,list_cards_for}`; `registry/reads.rs::{ListCardsRequest,ListCardsResponse,CardSummary}` | served OpenAPI and `pg_card_registration_route` | UI does not consume it | Extend `BrowserSession::api` call through `locals.wyrd` | None |
| Proven source | pinned `wyrd.ts::cards` and `CardDetail.test.ts` test `live UI Card inventory projection` | pinned browser case `live UI Card inventory` | uses old kinds/presentation assumptions | Reuse request/error orchestration only; map fourteen current kinds | None |

## Owners, Scope, Consumers, and Prohibited Changes

- Owners: existing Cards load/component, `WyrdClient::cards`, fixtures/tests, production UI journey.
- Consumers: exact Card links and later selectors.
- Prohibited: Workflow, invented totals/facets/global newest sorting, unbounded scans, browser API calls, or live-to-mock fallback.

## Approach

1. Reconcile pinned request mapping with current `ListCardsRequest/Response`.
2. Await the live facade in the existing server load and preserve structured errors.
3. Use URL-backed supported filters and opaque cursor; state when total is unknown.
4. Update explicit fixtures to fourteen kinds and pause for visual acceptance.

## Ordered Implementation Scenarios

### Scenario 1 — Bounded typed inventory

**Behavior.** SSR returns the authorized page in server order with exact Card links and `next_cursor`; page size is never presented as a total.

**RED.** Restore/adapt pinned test `live UI Card inventory projection`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardsInventory.test.ts -t 'live UI Card inventory projection'`.

**GREEN.** Extend the existing facade/load using current generated types; rerun the exact test.

**REFACTOR.** Share one presentation mapping between explicit fixtures and live results.

### Scenario 2 — Real-server success and refusal

**Behavior.** The real page supports cursor/filter/direct navigation; empty, denied, and unavailable are distinct with no fixture fallback.

**RED.** Restore/adapt pinned browser case `live UI Card inventory`. Run: `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Card inventory' mise run test:identity:journey`.

**GREEN.** Complete URL/error behavior and rerun both scenarios.

**REFACTOR.** Remove stale client-side count/sort assumptions.

## Acceptance Criteria

- Inventory portions of AC-002/007/008 pass; desktop/mobile evidence is accepted before TASK-004.

## Expected Write Set and Consumer Closure

- Cards route/load/component/types/fixtures/tests, existing server facade, generated contract source if required, Rust journey workload/host.
- Close row links, tenant routing, mock switch, pagination, error rendering, and back/forward state.

## Verification and Evidence

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardsInventory.test.ts
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Card inventory' mise run test:identity:journey
mise run codegen:check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
```

## Material Stop Conditions

- Required totals/facets need a new public registry contract; or current served schema conflicts with architecture.
- Human visual acceptance is withheld.

## Authority Links

- [Approved specification](../spec.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Repository rules](../../../../AGENTS.md)

