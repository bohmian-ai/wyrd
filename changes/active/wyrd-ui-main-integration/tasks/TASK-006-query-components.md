# TASK-006 — Query component family

Spec: [revision 9, approved](../spec.md). Status: Proposed.  
Dependencies: [TASK-003](TASK-003-shared-ui-foundation.md), [TASK-004](TASK-004-observe-components.md).  
Mapped obligations: REQ-030, REQ-014, REQ-022, REQ-020, AC-008, AC-011, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

## Outcome and Value

A complete read-only query workbench can be composed from reusable components in every host.

## Owners, Scope, Consumers, and Prohibited Changes

Own QueryEditor, TableCatalog and QueryDetails; reuse Chart and RecordTable. Include catalog discovery/description, explicit typed query parameters, cancel/rerun, table/chart presentation and exact Open in Query reproduction. No Service chart persistence or page-owned query shaping.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Bounded analytical reads | `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Bifrost.stream`; `crates/wyrd/wyrd-server/src/query/collect.rs::BoundedQuery` | `sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts`, `query-results.test.ts`; MCP `bifrost.rs::query_tool` | UI props and host parity missing | Consume foundation normalization and shared query owner | No second query service or transport |
| Historical query workflow | 7bfc3a8a `features/query/reads.server.ts::runQuery/readCatalog/describeTable` | Historical query routes and production-auth integration cases | Old object-row/truncation projection conflicts with current normalized contract | Reference authorized catalog/read flow; implement new pure loaders on foundation shape | No copy of old response/types or range injection |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Build catalog, editor and query-detail components and loaders.
2. Connect authored SQL/binds unchanged to the existing query floor and normalized results.
3. Finish cancellation, refusal, degraded and schema/display behavior across hosts.
4. Pass T1–T5 and individual gallery review.

## Ordered Implementation Scenarios

### Scenario 1 — Authored query is reproduced exactly

**Behavior.** Catalog selection, explicit binds, execute/cancel/rerun and chart/table views preserve SQL and typed inputs; range controls act only on declared range binds.

**RED.** Add query-family interaction and loader assertions; new components are absent. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-006: Authored query is reproduced exactly$'
```

**GREEN.** Reuse foundation executors and server admission, without rewriting SQL or deciding query policy. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Terminal-safe live workbench

**Behavior.** Real read-only SDK/MCP queries produce equivalent results; denied/malformed/non-SELECT/oversized/failed/cancelled reads never expose partial rows as success.

**RED.** Add focused family live/artifact proof using existing query write/flush/read setup. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-006: Terminal-safe live workbench$"'
```

**GREEN.** Retain server validation, structured codes and bounded rendering. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- All three components pass T1–T5 and individual visual/keyboard inspection.
- Errors and degraded evidence remain distinguishable; duplicate column names remain positional.
- General query range behavior follows explicit binds; chart links preserve executed SQL and bind values.

## Expected Write Set and Consumer Closure

Package Query family/loaders/catalog, gallery/fixtures/tests. No conventional Query route or backend endpoint. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` after focused tests. Run `mise run codegen:check` when catalog/generated inputs change. Record T1 states, T2 actual requests/shape, T3 view validation/render, T4 actual built AppBridge HTML, T5 real authorized Node/server data and T6 only for assembled pages. Data-free primitives have no invented loader/live proof. Preserve per-component fixture provenance and interaction/action-host evidence.

### Human visual checkpoint

Query and QueryChart; inspect editor, catalog, details and their table/chart composition. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../architecture/agent-rules.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-doctrine.mdx).
- [Execution](../../../../architecture/references/languages/implementation-execution.md), [TDD](../../../../architecture/references/languages/spec-driven-development.md), [testing](../../../../architecture/references/languages/testing-workflows.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).
