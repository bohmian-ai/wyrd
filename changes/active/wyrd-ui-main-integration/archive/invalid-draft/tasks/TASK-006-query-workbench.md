---
id: TASK-006
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 1
requirements: [REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-034, REQ-052]
invariants: [INV-002, INV-003, INV-004, INV-007, INV-008]
acceptance: [AC-004, AC-007, AC-008]
depends_on: [TASK-001, TASK-002]
parent_task: null
remediates: []
---

# TASK-006 — Read-only Query workbench

## Outcome and Value

The pinned Query workbench is re-established on current main through the canonical authenticated Bifrost client and production host.

## Source-Backed Reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| UI/state | pinned `features/query/{core.ts,QueryRunner,Catalog,ResultTable,QueryChart}` and Query routes | pinned `core.test.ts`; browser Query cases | absent on main | Port, removing obsolete correlation columns | Feature is required; no new transport |
| Reads | pinned `reads.server.ts::{runQuery,readCatalog,describeTable}` | `/v1/query`; `bifrost/routes.rs::{list,describe}`; `query/routes.rs::sync_query` | absent on main | Reuse TASK-001 `BrowserSession::query/api` | None |
| Image proof | pinned startup `authenticated_analytical_bff` | TASK-001 host | page must use same native addon in image | Extend existing proof with Query route | None |

## Owners, Scope, Consumers, and Prohibited Changes

- Owners: pinned Query feature/routes adapted to current shell/contracts. Prohibited: browser-direct Bifrost, mutation SQL, custom stream decoder, durable history, stale partial success, `subject_card_uid`/`owner_card_uid` roles.

## Approach

1. Port pinned feature/routes and currentize correlation/type semantics.
2. Preserve catalog/describe, bounded execution, cancellation, terminal errors, result schema, chart/table/details, copy/CSV, and tenant-local history.
3. Prove dev and official-image paths; obtain visual acceptance.

## Ordered Implementation Scenarios

### Scenario 1 — Local Query behavior

**Behavior.** URL state, catalog, SELECT execution, typed results/chart, CSV/copy, and per-tenant history work; refusal/terminal failure/cancel show no partial success.

**RED.** Restore pinned `core.test.ts`; run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/query/core.test.ts`.

**GREEN.** Port the pinned owners through TASK-001 boundaries; rerun the file.

**REFACTOR.** Keep one runner state machine and browser-local history.

### Scenario 2 — Real-server Query journey

**Behavior.** The real browser runs, limits, cancels, rejects, and renders results through the production BFF.

**RED.** Restore pinned cases; run `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Query run' mise run test:identity:journey` and the same command with `live UI Query limit and cancel`, `live UI Query refusals`, and `live UI Query results`.

**GREEN.** Complete route/server integration and rerun all four exact cases.

**REFACTOR.** Remove duplicated wire/result mappings.

## Acceptance Criteria

- AC-004/007/008 pass; official-image analytical proof remains green; desktop/mobile evidence is accepted.

## Expected Write Set and Consumer Closure

- Pinned Query feature/routes/tests, route registry/nav, TASK-001 server boundary, journey cases; image files only for a proven remaining gap.

## Verification and Evidence

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/query/core.test.ts
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Query results' mise run test:identity:journey
WYRD_STARTUP_FILTER=authenticated_analytical_bff mise run test:server:startup:prod
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
```

## Material Stop Conditions

- Current shared SDK cannot execute/decode the mounted query contract in Node; or a requested action requires mutation/new API.

## Authority Links

- [Approved specification](../spec.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Repository rules](../../../../AGENTS.md)

