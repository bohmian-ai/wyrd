---
id: TASK-010
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 1
requirements: [REQ-010, REQ-012, REQ-014, REQ-050, REQ-051, REQ-052, REQ-053]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-006, AC-007, AC-008, AC-009]
depends_on: [TASK-005, TASK-006, TASK-009]
parent_task: null
remediates: []
---

# TASK-010 — Integrated review environment and journey closure

## Outcome and Value

One repository-managed, production-shaped environment demonstrates every included route and required state without external credentials, while retaining the pinned branch's solved nginx, image, auth, route, seed, and browser integration.

## Source-Backed Reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Workload | pinned `review-seed.integration.test.ts`, `service-sim.ts`, `service-workload.ts` | pinned test `review seed: simulated Services with live traffic`; live UI cases | old kinds/result schema | Adapt public SDK/ingest/verification workload to current contracts | None |
| Browser host | current/pinned `identity_ui_e2e::production_ui_bff_journey`, pinned `mise.toml::run_ui` | all exact UI cases | current main lacks complete case matrix | Extend existing host only | None |
| Production host | TASK-001 bundle/nginx/entrypoint and pinned startup case | official image lane | final pages/states not collectively proven | Extend existing production-shaped proof | None |
| Visual evidence | accepted per-slice evidence and preserved canvas | human review | no final coherent route matrix | Capture final desktop/mobile states | No screenshot framework |

## Owners, Scope, Consumers, and Prohibited Changes

- Owners: existing deterministic workload, identity/startup hosts, repository environment, compact review evidence.
- Prohibited: direct DB insertion where public paths exist, fixed Service count as state proof, external providers, second compose stack, weakened auth, or modification of old branch/packet.

## Approach

1. Adapt the pinned workload to create current Cards, relationships, telemetry, queryable tables, generic results, drift features, sparse/dense states, and limited principals.
2. Require the filtered host to prove every case selects exactly one test.
3. Traverse every included route through the public proxy with direct-navigation and permission-negative cases.
4. Capture final desktop/mobile route/state evidence and run change-level verification.

## Ordered Implementation Scenarios

### Scenario 1 — Representative current data

**Behavior.** Supported public paths create every AC-009 state; setup asserts visibility before launching the UI.

**RED.** Adapt pinned test `review seed: simulated Services with live traffic`. Run: `WYRD_UI_INTEGRATION=1 mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/review-seed.integration.test.ts -t 'review seed: simulated Services with live traffic'`.

**GREEN.** Reuse the pinned workload with current public contracts and deterministic local providers.

**REFACTOR.** Remove obsolete Eval/Drift-kind/Workflow and fixed-scale assumptions.

### Scenario 2 — Integrated public-host journey

**Behavior.** Authenticated direct navigation and links cover all included routes, negative permissions, nginx route split, discovery/callback/origin, SvelteKit data requests, Rust APIs, Bifrost native packaging, and logout.

**RED.** Add planned case `live UI integrated review`. Run: `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI integrated review' mise run test:identity:journey`.

**GREEN.** Close only integration gaps and rerun TASK-001 production startup proof plus this case.

**REFACTOR.** Consolidate with existing hosts; retain no duplicate environment.

## Acceptance Criteria

- AC-001–009 pass; final visual matrix is accepted; migration evidence remains unchanged.

## Expected Write Set and Consumer Closure

- Pinned seed/workload adapted under existing test owners, journey/startup hosts, environment/run documentation, compact visual evidence; product code only for defects revealed by integration.
- Close all task routes, auth principals, proxy/image, Bifrost, generated contracts, mock/live separation.

## Verification and Evidence

```bash
WYRD_UI_INTEGRATION=1 mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/server/auth/review-seed.integration.test.ts
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI integrated review' mise run test:identity:journey
WYRD_STARTUP_FILTER=authenticated_analytical_bff mise run test:server:startup:prod
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run build
mise run codegen:check
```

At change review, use `mise run gate` only if the final cross-cutting write set has no complete capability gate, per AGENTS.md.

## Material Stop Conditions

- Required data cannot be created through supported public paths; production host differs irreconcilably from dev; external authority is required; or final visual acceptance is withheld.

## Authority Links

- [Approved specification](../spec.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)
- [Repository rules](../../../../AGENTS.md)

