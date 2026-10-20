---
id: TASK-004
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 1
requirements: [REQ-010, REQ-011, REQ-012, REQ-013, REQ-014, REQ-021, REQ-022, REQ-024, REQ-052]
invariants: [INV-001, INV-002, INV-004, INV-005, INV-006, INV-008]
acceptance: [AC-002, AC-007, AC-008]
depends_on: [TASK-003]
parent_task: null
remediates: []
---

# TASK-004 — Exact shared Card detail

## Outcome and Value

Every current Card kind has an exact versioned detail shell with identity band, Overview/Versions/YAML, shared work region, and compact relationship/metadata rail.

## Source-Backed Reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Detail host | current card `[uid]/+page.server.ts::load`, page component, workspace registry | `CardDetail.test.ts`, `workspace-registry.test.ts` | synchronous mock-only resolution and old composition | Extend existing host/registry | None |
| Exact reads | `cards/routes.rs::{get_card_http,get_card_by_ref_http,list_versions_http}`; `GetCardResponse/ListVersionsResponse` | OpenAPI/server route tests | route has UID without kind and needs bounded resolution; exact version must not float | Adapt pinned `WyrdClient::card`, `cardByUid`, `cardByRef` orchestration to current kinds | None |
| Proven presentation | pinned `card-live.ts::toYaml` and detail tests | existing YAML/copy/workspace consumers | useful exact YAML/version behavior is absent on main | Port source-valid presentation behavior | None |

## Owners, Scope, Consumers, and Prohibited Changes

- Owners: existing detail route/component/registry/facade/fixtures/tests.
- Consumers: all Card kinds and TASK-005/009/010.
- Prohibited: floating versions, name/label relationship inference, per-kind routes, full hashes as dominant content, or old Card kinds.

## Approach

1. Resolve UID to one current kind with bounded current-contract reads, then resolve selected versions exactly.
2. Map the full current Card envelope, relationships, timestamps, and available version values.
3. Reshape the existing shell to the approved canvas composition and exact YAML.
4. Prove missing/denied/version/relationship behavior and obtain visual acceptance.

## Ordered Implementation Scenarios

### Scenario 1 — Exact selected Card/version

**Behavior.** Direct URLs render the exact selected envelope; absent/denied versions never float.

**RED.** Extend existing test `selecting a prior version renders that version’s declared Spec`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts -t 'selecting a prior version renders that version’s declared Spec'`.

**GREEN.** Adapt the pinned exact-read orchestration to current contracts; rerun the existing 404 and generic-detail tests.

**REFACTOR.** Keep identity/version resolution in the existing server facade.

### Scenario 2 — Shared composition and real server

**Behavior.** Overview/Versions/YAML and relationship navigation render exact live data responsively with truthful failures.

**RED.** Add planned browser case `live UI generic Card detail`. Run: `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI generic Card detail' mise run test:identity:journey`.

**GREEN.** Complete the existing host/component and journey setup; rerun both scenarios.

**REFACTOR.** Remove repetitive field-box markup and stale casts.

## Acceptance Criteria

- Shared-detail portions of AC-002/007/008 pass; desktop/mobile evidence is accepted before TASK-005.

## Expected Write Set and Consumer Closure

- Detail route/component/styles/types, registry, facade/live projection, fixtures/tests, journey host/workload.
- Close inventory, versions, relationship links, Service/Verifier current presentations, raw declaration, denied/not-found behavior.

## Verification and Evidence

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/core/CardDetail.test.ts src/lib/features/cards/core/workspace-registry.test.ts
WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI generic Card detail' mise run test:identity:journey
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
```

## Material Stop Conditions

- Exact UID/kind/version resolution cannot be bounded by current APIs; or human visual acceptance is withheld.

## Authority Links

- [Approved specification](../spec.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Canvas reference](../references/canvas/canvas.json)

