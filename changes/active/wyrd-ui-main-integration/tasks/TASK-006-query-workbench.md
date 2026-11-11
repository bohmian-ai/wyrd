# TASK-006 — Query workbench

Spec: [revision 28, approved](../spec.md). Status: Proposed; History completion is conditional on the approved source/lifetime prerequisite below.
Dependencies: completed TASK-003 foundation, TASK-004 live shell/Observe/query inspector and TASK-027 view layout. Cards, unrelated component families and production MCP registration are not prerequisites.
Mapped obligations: REQ-014/020/022/030/034/062; AC-008/010/011/013/016/041; INV-001–INV-007 where applicable.
Consolidates former TASK-006 and TASK-019 in full, retaining the existing exact-query inspector as the starting owner rather than replacing it.

## Outcome and Value

A developer runs `mise run dev:full`, signs in and opens Query to discover an authorized table, inspect its schema, author a bounded read-only query with typed parameters and inspect its completed result as a table or chart. A query opened from Observe remains exactly reproducible, and Cancel prevents a superseded result from replacing the current view. Query components are usable independently in the package, gallery, custom apps and MCP Apps.

The existing page can inspect and rerun SQL supplied by Observe. This task expands that same owner into a workbench with table discovery, editing, details and result mapping; it does not add another query service.

## What Changes and What Must Stay True

Own `QueryEditor`, `TableCatalog` and `QueryDetails`, their portable loaders, schema/catalog entries, exports, view fixtures and host intents. Reuse `RecordTable`, `Chart`, existing range/value/result normalization and the existing executors. The conventional consumer remains `/t/[tenantKey]/query`; preserve current `sql`, JSON `params` and safe `back` navigation used by Observe. Direct navigation opens the workbench without requiring an Observe link. Table selection/search/descriptions and column insertion work there as soon as the new components exist.

The editor preserves authored SQL byte-for-byte when executing, copying, rerunning or opening it from a chart/Observe source. Preserve ordered `QueryParam` binds (null, boolean, number or string) separately from SQL; no interpolation. Range controls change only declared range binds and never inject predicates into general authored SQL. Current built-in query templates use authoritative `bifrost.*` names; obsolete authored SQL is submitted unchanged and fails visibly rather than being silently rewritten.

Table discovery and descriptions use current authorized SDK/MCP reads and preserve the returned status, schema, layout and available table/revision metadata. Unavailable metadata has a truthful unavailable/error state; no fabricated revision or schema. Search and insertion cannot smuggle unauthorized table details from a cached different tenant.

Run/rerun/cancel retain server-owned admission, authorization, read-only validation, deadlines and row/JSON-byte ceilings. Inputs and output stay bounded. A result is visible only after a valid successful or degraded terminal, with warnings/source-completion state preserved. Failed, oversized, malformed, cancelled or superseded work never becomes successful partial rows. Preserve positional rows, duplicate column names, order, nulls, integer precision and supported value encodings. QueryDetails reports the actually executed SQL/binds, completed result metadata and real refusal rather than the newly edited but unexecuted draft.

Table and chart are views of the same completed result; switching presentation does not issue another query or change binds. Chart mapping obeys supported column/shape rules and shows actionable validation errors. SQL copy and bounded CSV export operate on that executed query/result. CSV preserves positional columns, escaping and supported value representations; it cannot fetch unlimited additional rows or export a cancelled/failed partial stream. Service chart persistence belongs to TASK-010, not this workbench. There is no saved-view or Ask implementation.

Shared components render typed props and emit validated JSON-safe intents; portable loaders plan/shape; executors perform IO as the viewer. Pages own layout, URL/draft/selection state and dispatch completion only. Any existing page-owned request planning or result shaping being expanded moves to the shared Query owner in this task. Keep session credentials out of props/HTML/browser storage and preserve stable errors and server audit behavior. A failed prerequisite stops dependent reads; a newer run/input/range or route exit supersedes older work, even if remote cancellation cannot stop it.

**History prerequisite.** The Query board includes History, but neither this packet nor the current workbench defines its source, owner or lifetime. Revision 28 approves consolidation and delivery order, not a new history contract. Before implementing this checkpoint, obtain an explicitly approved decision defining: what is recorded (including SQL/bind handling), viewer/tenant visibility, persistence versus host-local lifetime, retention/clearing behavior, and how selecting a record reproduces its exact query/binds. Reuse an existing owner if the approved decision identifies one. Do not infer server audit storage, add a database table or silently choose browser localStorage/session memory. Independent catalog/editor/results work is startable; the History checkpoint and whole task cannot be accepted until its decision, implementation and focused proof are complete. No disabled control or fixture-only history closes it.

## Approved Canvases

These approved canvases define the visual target for this task. Follow their layout, component placement, hierarchy and interactions while using current Wyrd contracts and brand tokens. Apply the same component design in the gallery and the working UI. Record a separate comparison and human acceptance for each named canvas on desktop/mobile in light/dark.

- [Query.dc.html](../references/canvas/Query.dc.html)
- [QueryChart.dc.html](../references/canvas/QueryChart.dc.html)

If a required visual detail is absent or contradictory in these approved canvases, stop that affected visual work and ask the maintainer for clarification. Do not guess a layout, invent a replacement design, or treat a generic directory link as a visual specification.

## Approach

1. Reuse the current Query route, exact-query inspector and shared executor/result contracts; inspect current discovery/description behavior through both transports.
2. Implement each shared Query component with its catalog, gallery states and first live workbench region together.
3. Close typed query/parameter actions, cancellation/supersession, terminal-safe result handling and table/chart/export behavior across hosts.
4. Resolve the named History prerequisite before its implementation; do not let it prevent independent approved work or disappear from completion.
5. Ensure normal development setup supplies a discoverable authorized table and useful bounded query result, then prove real-host/browser behavior and both Query boards.

## Ordered Implementation Scenarios

### Scenario 1 — Catalog discovery and authoring preserve current contracts

**Behavior.** A viewer searches authorized tables, describes one, inserts a column into an editable query and supplies typed binds. Metadata matches the actual table description, including unavailable cases. Execute/copy/rerun preserve authored SQL and bind order; an Observe link prefills the exact executed query and retains its safe return path. General SQL receives no automatic time predicate or legacy-name rewrite.

**RED.** Add the planned [authoring component proof](#authoring-component-proof). Assert actual discovery/description requests, draft-versus-executed state, SQL/bind preservation and invalid parameter/state behavior; validate/render all new component view entries. Missing catalog/editor/detail behavior must fail in the existing renderer/loader runner.

**GREEN.** Build the shared Query projections and workbench composition over current read owners. Retain the existing exact-query inspector workflow and pass this proof.

**REFACTOR.** Reuse portable request/result owners and existing action validation; do not add a query client, metadata store or SQL parser to compensate for a server contract.

### Scenario 2 — Execution is bounded, cancellable and terminal-safe

**Behavior.** A real SDK and MCP execution with identical SQL/binds returns the same normalized completed result. Nulls, duplicate names and exact supported values survive. Server denials, malformed parameters, non-SELECT statements, row/byte ceilings, failed streams and cancellation retain their refusal semantics without successful partial results. Degraded terminal results keep their warnings/source-completion status. Rerunning or leaving the page supersedes outstanding work and late responses cannot replace current state.

**RED.** Add the planned [terminal-state component proof](#terminal-state-component-proof) for state/cancellation/late-response cases and [authorized query host proof](#authorized-query-host-proof) for actual SDK/MCP permission/admission/terminal behavior and built AppBridge rendering. Do not fake a server refusal as an arbitrary empty query result.

**GREEN.** Connect the shared Query loader and host actions to the existing bounded executor/stream; make only the minimal consumer correction needed. Rerun scenarios 1–2 and affected exact-query inspector assertions.

**REFACTOR.** Remove duplicated page result shaping and reuse established bounded collection/cancellation. Do not introduce another stream lifecycle, retry policy or query admission owner.

### Scenario 3 — One completed result supports table, chart and export

**Behavior.** The viewer switches between table and supported chart mappings without re-executing the query. QueryDetails continues to identify the executed SQL/binds/terminal even while the draft changes. Invalid chart mapping is an in-view error. SQL copy reflects the executed query and CSV contains only the bounded completed rows with correct positional headers and escaping. Cancelled/failed runs have no successful result to export.

**RED.** Add the planned [result presentation proof](#result-presentation-proof), using duplicate columns, nulls, encoded large values, CSV-special characters, unsuitable chart columns and a changed draft. Assert no new read on view/export changes and no export of failed partial output.

**GREEN.** Compose existing RecordTable/Chart using the same stored completed result and implement authorized host completion for copy/export. Rerun scenarios 1–3.

**REFACTOR.** Reuse the existing normalized result and supported chart schema. Do not convert positional rows to name-keyed objects or add Service/saved-view persistence.

### Scenario 4 — History follows its approved owner and lifetime

**Behavior.** After the named History prerequisite is approved, successful/failed/cancelled record inclusion, isolation, lifetime and clearing follow that exact contract. Selecting a recorded item restores its recorded SQL/binds without automatic SQL changes; editing it does not mutate an older record. A source failure remains visible and never exposes another viewer's or tenant's record.

**RED.** Only after approval, add the planned [History proof](#history-proof), with cases for its approved lifetime/visibility and exact reproduction. Record the decision and assertions in this task before execution. An absent material contract is a stop condition, not license to choose storage in the test.

**GREEN.** Implement the minimum approved History behavior at its named owner and connect the workbench. Rerun scenarios 1–4 and the decision's actual integration seam where required.

**REFACTOR.** Reuse the approved owner and current query reproduction path. Do not add a parallel history source or cross-host persistence abstraction.

### Scenario 5 — The authenticated and normal development workbench is usable

**Behavior.** A real browser signs in through public auth, discovers a table, describes/inserts a column, edits SQL/binds, runs/cancels/reruns, switches result presentation, inspects details, copies SQL and exports bounded CSV. Denied/error/cross-tenant and obsolete-query flows remain truthful. Normal `dev:full` exposes this workflow without hidden seeding steps and preserves Observe's Open in Query/Back behavior.

**RED.** Add the planned [authenticated workbench proof](#authenticated-workbench-proof) against production BFF/Rust server and extend the existing [development lifecycle proof](#development-lifecycle-proof) with catalog/run/chart readiness. A browser must exercise actual controls; fixture PageData/imported handlers cannot satisfy the journey.

**GREEN.** Close route, shared package, host intents and normal development data together. Rerun the earlier changed scenarios and the inspector regression. Keep whole-task completion open until the History prerequisite and its checkpoint pass.

**REFACTOR.** Reuse existing public sign-in, deployed BFF, browser and actual-launch owners; do not add another runtime environment or fixture-backed production path.

## Acceptance Criteria

| Obligation | Observation and proof | Violation |
|---|---|---|
| REQ-014/030; AC-008/011 | Scenario 1 and catalog/editor/details gallery plus authorized host/browser proof show current discovery/descriptions and typed unchanged SQL/binds. | Unauthorized metadata, silent SQL rewrite or app-only query component. |
| REQ-022/030; AC-008/016; INV-004/006/007 | Scenario 2 exercises real refusals, terminal safety, bounds, cancellation and late-result suppression; obsolete SQL fails unchanged. | Partial success after failure/limit, lost degraded evidence, stale overwrite or duplicate-name collapse. |
| REQ-014; AC-008 | Scenario 3 proves table/chart equivalence, exact details/copy and bounded correctly escaped positional CSV without another read. | Chart/query drift, export widening or draft shown as executed SQL. |
| Query board History obligation; REQ-020; AC-008/010 | Scenario 4 follows a separately approved source/lifetime/visibility decision and its actual seam proof. | Invented persistence or silently omitted History region. |
| REQ-030; AC-011 | All three components have T1 states, T2 requests/shaping, T3 view/catalog, T4 actual built AppBridge and applicable T5 real authorized Node/MCP evidence; custom-app package use remains valid. | Mock-only component completion, missing host action or browser native-addon import. |
| REQ-034/062; AC-013/041; applicable invariants | T6 and normal `dev:full` show live Query discovery/edit/run/chart/export and preserved Observe navigation under real auth/tenant permissions. | Page merely opens, undocumented manual seed step, fixture data or cross-tenant leak. |
| REQ-020; AC-010 | Individually inspect QueryEditor, TableCatalog, QueryDetails and both Query/QueryChart page boards in light/dark desktop/mobile with keyboard/focus/touch/overflow. | One combined screenshot substituted for per-component/page acceptance. |

## Implementation Reference

UI-relative paths below are rooted at `crates/wyrd/wyrd-server/wyrd-ui/`; they identify owners, not a private file allowlist.

| Existing owner and inspected consumers | Gap and selected extension |
|---|---|
| `src/routes/t/[tenantKey]/query/+page.server.ts::actions.run`, `query/+page.svelte`; `src/lib/observe-host.ts::step`; existing browser `authenticated traces and spans` covers Open in Query/rerun/Back | A bounded exact-query inspector already works. Extend this route to consume shared Query components/loaders instead of creating another page or retaining new page-owned planning/shaping. |
| `packages/ui/src/lib/contract.ts::WireRequest/QueryResult/Executor`; `node.ts::nodeExecutor`; `mcp.ts::mcpExecutor`; existing `bifrost.query/list_tables/describe_table` mappings | Existing read/terminal/credential owner supports discovery and bounded execution. Add portable Query projections and host intents; preserve the closed approved read floor and generated wire types. |
| `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Bifrost.stream/listTables/describeTable`, `BifrostQueryStream.collect`; `tests/integration/query-bifrost.test.ts`, `query-results.test.ts`; Rust `src/query/collect.rs::BoundedQuery` | Current public owner supplies table metadata, exact binds and positional bounded results. Extend UI consumers rather than adding transport, query policy or result normalization machinery. |
| `packages/ui/src/lib/schemas.ts::ChartInputSchema`, `loaders.ts::loadChart/chartParams/loadPanel`; `src/lib/ui-library.test.ts`, `ui-library.integration.test.ts::appBridgeView/mcpOver` | Reuse chart/result/action conventions and built artifact/real MCP parity setup. Add three component schemas/catalog/exports/fixture view docs and shared loaders. |
| `src/lib/ui-pages.integration.test.ts::startBff/signIn/humanSignIn/devFull`; `scripts/dev/example-app.sh::setup/batch`; `mise.toml::dev:full` | Real deployment/browser/normal dev launch already exist. Extend their proof/data readiness; do not invent a fixture-only mode or another runner. |

Likely changes are shared Query components/loaders/schema/catalog sources and generated outputs, the existing Query route and host action adapters, gallery/fixtures/focused tests and normal development data if current retained signals are insufficient. Preserve server admission/security and all directly affected package/custom-app/MCP consumers. Historical UI reads/boards supply evidence only; no deleted Query implementation returns.

## Verification and Evidence

All new titles below are **planned**. Confirm the actual selector after adding each test and before RED. Current `package.json::test` builds package/artifact and runs Vitest; `vite.config.ts` includes these files and gates integration with existing `WYRD_UI_INTEGRATION`. Skipped/no-selected cases are not proof.

Live prerequisites use [verification owners](../references/verification.md): current SDK/testing/server build, existing workspace dependencies and Playwright Chromium. Reuse `serverTest` and built BFF/public sign-in; record exact corrected commands when owner layout changes. Build Cargo prerequisites sequentially in the shared checkout.

### Authoring component proof

T1–T3 state/input/catalog/discovery and exact SQL/bind assertions:
```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^query authoring preserves catalog metadata sql and typed binds$'
```

### Terminal-state component proof

T1/T2 cancellation/terminal/supersession cases:
```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^query execution discards failed cancelled and superseded partial results$'
```

### Result presentation proof

Table/chart/details/copy/CSV interaction assertions using one completed normalized result:
```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^query table chart details and export share one completed result$'
```

### History proof

Planned focused behavior title, implemented only after its approved source/lifetime decision is recorded. Add the actual owner's integration proof too if that decision crosses a persisted/server boundary; do not claim this component test proves persistence.
```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.test.ts -t '^query history reproduces records within its approved visibility and lifetime$'
```

### Authorized query host proof

T4 built AppBridge plus T5 actual SDK/MCP discovery/query parity, populated/empty/denied, limits/terminal negatives and another tenant:
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^query components preserve authorized discovery and terminal results across hosts$"'
```

### Authenticated workbench proof

T6 real production-BFF/browser controls and resulting authorized reads, chart/export/details, cancellation, failures and cross-tenant absence. Extend with the approved History visibility/lifetime cases once that prerequisite is supplied.
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated query discovery authoring and result presentation$"'
```

### Existing inspector regression

Unchanged behavior uses regression proof rather than a manufactured RED:
```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated traces and spans$"'
```

### Development lifecycle proof

Extend this **existing** actual-launch selector with catalog/run/chart readiness and exact-query navigation assertions; retain its failure/cleanup checks:
```bash
mise exec -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t '^development environment lifecycle$'
```

This owns its development Postgres/services and requires free normal dev ports, isolated config and the launcher's existing prerequisites. Do not start another managed server around it.

### Scoped checks and human checkpoints

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run codegen:check
mise run dev:full
```

Codegen is required when catalog/generated inputs change, as expected here. Apply existing format/lint checks to changed owners and add exact focused owner checks if SDK/Rust code actually changes. Integrated change review owns broad aggregate and combined surface journeys.

For manual inspection, sign in with the launcher's printed development login, open Query directly, discover and describe a seeded `bifrost` table, insert a column, run bounded SQL with explicit binds and view a suitable result as table and chart. Copy SQL/export CSV, cancel/rerun, then use Open in Query from Observe and return without losing its context. Record the exact representative table/query/binds and expected shape in implementation evidence. Use retained canonical workload data where sufficient; extend the existing production setup/readback when needed, never write protected result tables or substitute gallery fixtures. History is a separate required checkpoint after its decision; it cannot be marked absent by design without new approval.

Inspect each new component through `/_gallery` with T1 states and validate Query then QueryChart separately against their current boards. Record source region/screenshot, implementation screenshot, theme/state/viewport and individual human acceptance, including keyboard/focus/touch, desktop/mobile, long SQL/results and narrow overflow. Required human acceptance precedes the next dependent slice; it is not replaced by a running page or automated screenshot.

## Material Stop Conditions

Stop History implementation for its missing approved source/lifetime/visibility contract; independent work may continue but whole-task acceptance remains open. Escalate any newly required public operation, changed query/admission/security semantics or durable ownership. Do not invent a history store, rewrite obsolete authored SQL, fabricate successful partial results or weaken negative/cross-host/browser proof. Private component organization, bounded presentation choices and equivalent exact-selector corrections remain implementation-owned. Missing credentials/platform/human proof remains unverified rather than PASS.

## Authority Links

- [Approved spec](../spec.md), [plan and ownership map](../plan.md), [verification owners](../references/verification.md), [approved canvases](#approved-canvases).
- [Repository rules](../../../../AGENTS.md), [Bifrost authority](../../../../architecture/bifrost-design.md), [security posture](../../../../architecture/wyrd-security-posture.md).
- [Execution](../../../../changes/implementation-execution.md), [spec-driven development](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md), [Wyrd UI skill](../../../../.agents/skills/wyrd-ui/SKILL.md).
