# TASK-020 — Traces and span investigation page

Spec: [revision 9, approved](../spec.md). Status: Proposed; platform prerequisite holds completion.  
Dependencies: [TASK-012](TASK-012-canvas-shell.md), [TASK-004](TASK-004-observe-components.md).  
Mapped obligations: REQ-015, REQ-028, REQ-020, AC-008, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

External dependency: REQ-023 for actual Service filter; no service_name stand-in. This task does not authorize implementation of that platform prerequisite. Contract-independent work may proceed, but fixtures/unavailable controls cannot close it.

## Outcome and Value

A user filters traces or flat spans, opens an exact trace drawer and investigates its waterfall and span details without losing list position.

## Owners, Scope, Consumers, and Prohibited Changes

Assemble only this outcome from completed Chart, StatStrip, FacetRail, RecordTable, TraceWaterfall and RecordDetail. Page/server load functions invoke shared loaders; no SQL, joins, dependent-read sequencing or domain shaping in pages. Reuse session-bound Node executor and shell. Root spans are trace rows; fetch counts for visible rows only. No trace-summary table. Live pauses during scroll/hover/open trace and shows the new-row notice. Filters write to WHERE; span names come from data. Observe has no verification verdict content. No historical components/layouts/types or fixture production fallback.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Bounded analytical reads | `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Bifrost.stream`; `crates/wyrd/wyrd-server/src/query/collect.rs::BoundedQuery` | `sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts`, `query-results.test.ts`; MCP `bifrost.rs::query_tool` | UI props and host parity missing | Consume foundation normalization and shared query owner | No second query service or transport |
| Page composition | TASK-003–011 proposed public package/renderer/executors and TASK-012 shell; current `crates/wyrd/wyrd-server/wyrd-ui/src/hooks.server.ts::handle` and BrowserSessions own auth | Current retained login/settings loads; SDK tests/support/server.ts and existing production-auth driver own real server/BFF setup | Approved page absent on committed main | Add layout/loader consumer and focused T6 journey | No new transport, state controller or duplicate component owner |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Compose finished components to the named canvas with only host/layout state in the app.
2. Connect session-bound loaders and declared navigation/actions; preserve exact identities, range and URL/drawer state.
3. Prove focused T6 through real production BFF/SDK/server, including empty, denied/error and cross-tenant cases.
4. Run the page visual checkpoint and wait for human acceptance before its dependent slice.

## Ordered Implementation Scenarios

### Scenario 1 — Page interaction and composition

**Behavior.** A user filters traces or flat spans, opens an exact trace drawer and investigates its waterfall and span details without losing list position. Root spans are trace rows; fetch counts for visible rows only. No trace-summary table. Live pauses during scroll/hover/open trace and shows the new-row notice. Filters write to WHERE; span names come from data. Observe has no verification verdict content.

**RED.** Add the task-focused page interaction regression; approved route composition is absent after REQ-032 deletion. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts -t '^TASK-020: Page interaction and composition$'
```

**GREEN.** Compose existing package components/loaders and shell without reopening their contracts. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Real authenticated page journey

**Behavior.** T6 drives sign-in/session-bound production BFF loads to real SDK/server data and back to the page; populated, empty, denied/error and cross-tenant cases remain truthful. Required actions and links preserve exact identity.

**RED.** Add the focused page live journey against existing @wyrd/testing/Postgres and production BFF; fixture page props are insufficient. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^TASK-020: Real authenticated page journey$"'
```

**GREEN.** Use existing auth and public server workflows, retaining structured errors and no credential serialization. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- Both focused scenarios pass and all page-specific requirements above are covered.
- No page-level query/shaping code, mock fallback or tenant/credential leak; existing login/settings remain valid.
- Human accepts this page on desktop/mobile in light/dark, including populated/empty/error, keyboard/focus, touch, long content and overflow.

## Expected Write Set and Consumer Closure

Only relevant app route/layout/server load, host action/navigation adapters and focused page tests/fixtures; extend shared library only for a demonstrated missed approved contract and rerun its affected proofs. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` after focused tests. Run `mise run codegen:check` when catalog/generated inputs change. Record T1 states, T2 actual requests/shape, T3 view validation/render, T4 actual built AppBridge HTML, T5 real authorized Node/server data and T6 only for assembled pages. Data-free primitives have no invented loader/live proof. Preserve per-component fixture provenance and interaction/action-host evidence.

### Human visual checkpoint

Main, Spans and TraceDrawer as one trace-investigation workflow; inspect each mode/drawer checkpoint before accepting it. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md).
- [Execution](../../../../changes/implementation-execution.md), [TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
