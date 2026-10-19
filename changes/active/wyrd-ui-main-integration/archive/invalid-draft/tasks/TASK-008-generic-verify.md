---
id: TASK-008
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 1
requirements: [REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-040, REQ-041, REQ-042, REQ-043, REQ-052]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-008]
acceptance: [AC-005, AC-007, AC-008]
depends_on: [TASK-001, TASK-002, TASK-004]
parent_task: null
remediates: []
---

# TASK-008 — Generic Verify overview and result detail

## Outcome and Value

`/t/{tenant}/verify` exposes current generic verification results, exact detail, drift-requirement feature expansion, and sensitive-outcome denial without restoring old Eval/Drift ontologies.

## Source-Backed Reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Result contract | `VerificationExecutionState`, `VerificationResult`, `VerificationOutcome`, `RequirementResult`; `verification/routes.rs::{get_execution,get_binding}` | current verification route tests | no UI | Project current contracts and exact identities | New page required, no new durable service |
| Analytical rows | `ResultsTable::arrow_fields`, `ResultFeaturesTable::arrow_fields` | Bifrost table tests/design | no UI query; sensitive `outcome` needs separate permission | Query metadata separately; explicitly project protected outcome; join features on result+requirement | None |
| Old UI evidence | pinned evaluation/drift routes and `observe-live.ts` interaction patterns | pinned old tasks/tests | vocabulary/columns are invalid (`implementation`, old subject semantics) | Reuse visual interaction only, not types/SQL/routes | None |
| Verifier Card | current Verifier workspace/registry | `CardDetail.test.ts` test `Verifier renders...` | must align to generic model | Extend existing presentation | None |

## Owners, Scope, Consumers, and Prohibited Changes

- Owners: new Verify route/feature under existing UI conventions, server-only projection, existing Verifier workspace, tests/journey. Prohibited: Eval/Task/Drift routes or result types, old columns, status/verdict conflation, eager protected outcome, manual window-Verifier run UI.

## Approach

1. Build one bounded current-schema overview and one generic detail model.
2. Preserve exact execution/application Run/Verifier/subject/owner/binding/window identities and declared requirement order.
3. Gate outcome detail before retrieval; expand feature rows only beneath their drift requirement.
4. Align Verifier Card and obtain passed/failed/inconclusive/denied visual acceptance.

## Ordered Implementation Scenarios

### Scenario 1 — Overview and exact detail

**Behavior.** Supported filters/grouping/paging distinguish execution status from verdict and open one exact result with current requirement/step/evidence/provenance/attempt/timing data.

**RED.** Add planned test `generic Verify keeps execution status separate from verdict and opens exact detail`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/verify/VerifyJourney.test.ts -t 'generic Verify keeps execution status separate from verdict and opens exact detail'`.

**GREEN.** Add the minimum current-schema route/projection/presentation; rerun the exact test.

**REFACTOR.** Keep one result model shared with later summaries.

### Scenario 2 — Drift expansion and sensitive denial

**Behavior.** Feature detail joins by result+requirement; missing baseline, insufficient data, inconclusive and failed differ; denied outcome retains authorized metadata.

**RED.** Add planned test `generic Verify scopes drift features and preserves metadata when outcome is denied`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/verify/VerifyJourney.test.ts -t 'generic Verify scopes drift features and preserves metadata when outcome is denied'`.

**GREEN.** Add authorized explicit outcome/feature reads and run planned browser case: `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI generic Verify' mise run test:identity:journey`.

**REFACTOR.** Remove migrated old Eval/Drift routes/nav/fixtures and align Verifier Card.

## Acceptance Criteria

- AC-005/007/008 pass; no removed vocabulary remains in migrated UI; desktop/mobile evidence is accepted.

## Expected Write Set and Consumer Closure

- Verify routes/feature/types/styles/tests, facade/projection, explicit fixtures, Verifier workspace, nav and journey workload/case. Close Card/Query/Observe links, sensitive permission, generated contracts if consumed.

## Verification and Evidence

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/verify/VerifyJourney.test.ts
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI generic Verify' mise run test:identity:journey
mise run codegen:check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
```

## Material Stop Conditions

- Current contracts cannot expose required data; permission cannot gate before payload retrieval; or mapping requires guessed identity.

## Authority Links

- [Approved specification](../spec.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Bifrost design](../../../../architecture/bifrost-design.md)

