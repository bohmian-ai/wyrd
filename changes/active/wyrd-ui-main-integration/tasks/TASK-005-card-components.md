# TASK-005 — Card browsing and definition component family

Spec: [revision 9, approved](../spec.md). Status: Proposed; platform prerequisite holds completion.  
Dependencies: [TASK-003](TASK-003-shared-ui-foundation.md).  
Mapped obligations: REQ-030, REQ-024, REQ-020, AC-006, AC-007, AC-011, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

External dependency: Only if required ArtifactTable actions lack a current SDK/MCP projection: obtain approved platform action contract before calling that action complete. This task does not authorize implementation of that platform prerequisite. Contract-independent work may proceed, but fixtures/unavailable controls cannot close it.

## Outcome and Value

Exact Card identity, relationships and kind-specific declarative content can be composed in any of the four hosts, independently of page layouts.

## Owners, Scope, Consumers, and Prohibited Changes

Own RelationshipList, CardFacts, CompositionGraph, PromptMessages, PromptVariables, UsedBy, VersionDiff, ModelInterface, ArtifactTable, CardInventory and CardInventorySummary. Reuse CardHeader from TASK-003. Current Card/spec/artifact contracts win over historical layouts and handwritten types. No Prompt telemetry strip or client-side lineage owner.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Exact Card reads | `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Cards.get`, `Cards.list`; server `src/mcp/cards.rs::GET_BY_REF` | `sdks/wyrd-sdk-ts/wyrd/tests/support/server.ts::registered`; current Card envelopes | No portable UI projection | Extend the approved component loaders over these reads | No new registry, identity lookup or lineage owner |
| Kind content | `crates/wyrd-spec/src/card/agent.rs::AgentSpec`, `prompt/spec.rs::PromptSpec`, `model.rs::ModelSpec`, `data.rs::DataSpec`, `service.rs::ServiceSpec` | SDK registered Card reads; historical server/card-live.ts::liveAgent/livePrompt/liveModel at 7bfc3a8a | No reusable canvas-aligned content projections | New generated-type-based components and pure loaders over current reads | Historical functions are evidence only; no old types/components copied |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Complete exact Card discovery/version paging and portable declarative projections.
2. Render all inventory members, including native/portable Prompt content, exact graph references, artifacts and truthful recorded badges.
3. Declare navigation/copy/artifact intents and finish host completion/error/denial behavior where the existing capability supports them.
4. Pass T1–T5 and inspect each member in the gallery.

## Ordered Implementation Scenarios

### Scenario 1 — Exact and current Card content

**Behavior.** Filters/page cursors, version changes, graph links, Prompt forms/diffs and Model signature/artifacts preserve exact CardRefs and current schema; badges only project recorded facts.

**RED.** Add family state/projection/identity tests; the shared components do not exist. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-005: Exact and current Card content$'
```

**GREEN.** Build family components/loaders with generated wire types and server-derived relationships. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Real registry and host actions

**Behavior.** T1–T5 prove populated/empty/denied and exact-version behavior across hosts; artifact/copy/navigation actions complete or expose their real stable refusal without credential leaks.

**RED.** Add focused registry-to-component journey; include underprivileged and another-tenant access. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-005: Real registry and host actions$"'
```

**GREEN.** Reuse SDK registration/storage contracts; a missing artifact action projection remains an explicit dependency, not fake success. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- All eleven named components are fully rendered/tested/individually accepted, including long Prompt and graph content on mobile.
- No hand-authored domain types, stale workspace layouts or silent latest-version substitution.
- Artifact production completion requires the actual supported host projections; do not widen the closed read union without approved consumer closure.

## Expected Write Set and Consumer Closure

Package Card family, loaders/input catalog, gallery/fixtures/tests, necessary generated roots and existing artifact consumer adapters only. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` after focused tests. Run `mise run codegen:check` when catalog/generated inputs change. Record T1 states, T2 actual requests/shape, T3 view validation/render, T4 actual built AppBridge HTML, T5 real authorized Node/server data and T6 only for assembled pages. Data-free primitives have no invented loader/live proof. Preserve per-component fixture provenance and interaction/action-host evidence.

### Human visual checkpoint

Cards content plus Agent, Prompt, Model, Data and Service boards. Inspect every family member, separately from later page acceptance. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md).
- [Execution](../../../../changes/implementation-execution.md), [TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
