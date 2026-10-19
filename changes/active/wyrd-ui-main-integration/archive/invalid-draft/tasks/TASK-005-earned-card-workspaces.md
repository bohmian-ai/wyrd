---
id: TASK-005
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-main-integration
spec_revision: 1
requirements: [REQ-021, REQ-023, REQ-024, REQ-052]
invariants: [INV-001, INV-002, INV-005, INV-006, INV-008]
acceptance: [AC-002, AC-007, AC-008]
depends_on: [TASK-004]
parent_task: null
remediates: []
---

# TASK-005 — Agent, Prompt, and Model Card workspaces

## Outcome and Value

Agent, Prompt, and Model Cards gain their earned canvas-directed work regions inside the accepted shared detail shell. Each workspace is presented to the user for visual acceptance before the next scenario proceeds.

## Source-Backed Reuse

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Agent | pinned `AgentWorkspace.svelte`, `card-live.ts::liveAgent` | pinned test `Agent renders its composition...`; browser `live UI Agent detail` | absent on main; old evidence fields stale | Port declaration/relationship composition; use current exact evidence only | Existing registry earns this workspace |
| Prompt | pinned `PromptWorkspace.svelte`, `livePrompt`, `promptMessages`, `promptModel` | pinned Prompt component/browser tests | absent on main | Port current-schema message/settings presentation | Same |
| Model | pinned `ModelWorkspace.svelte`, `liveModel`, `shorten` | pinned Model component/browser tests | absent on main; artifact actions unsupported | Port read-only interface/artifact presentation; omit unsupported actions | Same |
| Registry | current `workspace-registry.ts::{discoverWorkspaces,buildWorkspaceRegistry}` | current registry tests | only Service/Verifier are registered | Register the three exact current kinds | None |

## Owners, Scope, Consumers, and Prohibited Changes

- Owners: existing workspace registry plus pinned workspace components/projections adapted under current Card types.
- Prohibited: inferred relationships, prompt execution/editing, artifact download without a server contract, Workflow/SubAgent/Skill kinds, or old verification columns.

## Approach

1. Port one workspace at a time into the shared host: Agent, then Prompt, then Model.
2. Map only current declarations and exact server-derived relationships/evidence.
3. After each scenario, run its exact journey and pause for desktop/mobile visual acceptance.

## Ordered Implementation Scenarios

### Scenario 1 — Agent workspace

**Behavior.** Exact Prompt → Agent → Model composition, run config, invocations, and attributable bindings/evidence render without inference.

**RED.** Restore pinned test `Agent renders its composition, run config, vitals, invocations and verification`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/component-workspaces.test.ts -t 'Agent renders its composition, run config, vitals, invocations and verification'`.

**GREEN.** Port/adapt the pinned Agent owner and `liveAgent`; run `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Agent detail' mise run test:identity:journey`.

**REFACTOR.** Remove old kind/result assumptions; obtain visual acceptance before Scenario 2.

### Scenario 2 — Prompt workspace

**Behavior.** Ordered messages, variables, model settings, raw request disclosure, and exact consumers render safely.

**RED.** Restore pinned test `Prompt renders messages first, one-line variables and the raw request behind a disclosure`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/component-workspaces.test.ts -t 'Prompt renders messages first, one-line variables and the raw request behind a disclosure'`.

**GREEN.** Port/adapt the pinned Prompt owner; run `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Prompt detail' mise run test:identity:journey`.

**REFACTOR.** Keep transport out of the component; obtain visual acceptance before Scenario 3.

### Scenario 3 — Model workspace

**Behavior.** Interface, artifacts, production behavior, consumers, and attributable evidence render with compact hashes and no unsupported action.

**RED.** Restore pinned test `Model renders inputs → model → outputs and its artifacts with short hashes`. Run: `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/component-workspaces.test.ts -t 'Model renders inputs → model → outputs and its artifacts with short hashes'`.

**GREEN.** Port/adapt the pinned Model owner; run `WYRD_IDENTITY_TARGET=ui WYRD_IDENTITY_FILTER='live UI Model detail' mise run test:identity:journey`.

**REFACTOR.** Reuse shared Card primitives and obtain final visual acceptance.

## Acceptance Criteria

- Workspace portions of AC-002/007/008 pass; each visual checkpoint is recorded separately.

## Expected Write Set and Consumer Closure

- Three pinned workspace component/index owners, registry/tests, current Card presentation types, live projection/fixtures, existing detail route and journey workload.
- Close exact Card/version, relationship links, evidence permission/absence, responsive long content.

## Verification and Evidence

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/component-workspaces.test.ts src/lib/features/cards/core/workspace-registry.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui run check
```

## Material Stop Conditions

- A workspace claim needs inferred identity, a new public action, or removed vocabulary; or a visual checkpoint is withheld.

## Authority Links

- [Approved specification](../spec.md)
- [Wyrd design](../../../../architecture/wyrd-design.md)
- [Canvas reference](../references/canvas/canvas.json)

