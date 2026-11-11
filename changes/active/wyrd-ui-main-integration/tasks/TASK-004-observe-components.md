# TASK-004 — Observe component family

Spec: [revision 18, approved](../spec.md). Status: Complete; revision 18 live Observe closure is outstanding.
Dependencies: [TASK-003](TASK-003-shared-ui-foundation.md).
Mapped obligations: REQ-030, REQ-022, REQ-028, REQ-020, AC-008, AC-011, AC-010; all INV-001–INV-007 apply, with TASK-001-specific exclusions only in TASK-001.

REQ-023 is delivered on main: Service filters use `workload_card_uid` and component breakdowns use `subject_card_uid` on `bifrost.*` tables.

## Outcome and Value

All reusable Observe components are complete and independently inspectable in the gallery before Observe pages exist.

## Owners, Scope, Consumers, and Prohibited Changes

Own StatStrip, FacetRail, RecordTable, TraceWaterfall and RecordDetail. Reuse the completed Chart and primitives. Trace/list/detail/log/metric variants belong to these component contracts; shell controls and conventional pages do not. Service correlation variants use main's `workload_card_uid`.

Use committed main `8651b96fd`, the approved spec and current canvas. Historical `wyrd-server-ui` at `7bfc3a8a` supplies read/state/test evidence only. Current uncommitted trace changes are unrelated and must be preserved. Package components render typed props; portable loaders plan/shape; executor/host boundaries own credentials, authorized IO and action execution. Preserve tenant isolation, stable errors and server audit behavior. No speculative framework, test-only configuration or replacement selectors.

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Bounded analytical reads | `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Bifrost.stream`; `crates/wyrd/wyrd-server/src/query/collect.rs::BoundedQuery` | `sdks/wyrd-sdk-ts/wyrd/tests/integration/query-bifrost.test.ts`, `query-results.test.ts`; MCP `bifrost.rs::query_tool` | UI props and host parity missing | Consume foundation normalization and shared query owner | No second query service or transport |
| Canonical signals | `crates/vala/vala-bifrost-redux/src/tables/traces/spans.rs::SpansTable`, `logs/records.rs::RecordsTable`, `metrics/points.rs::PointsTable`, `gateway/calls.rs::CallsTable` | SDK integration observation-surface/observe-a-run sources; historical features/observe/core/filter-state.ts is reference only | New reusable signal loaders and interactions absent | Query current canonical schemas with portable foundation loaders | No trace-summary table, per-kind emit model or UI-owned telemetry |

Paths under `src/mcp`, `src/query` or `tests/integration` described as server owners are rooted at `crates/wyrd/wyrd-server/`. Proposed TASK dependencies are not capabilities already present on main.

## Approach

1. Implement family props/loaders/catalog and fixture/gallery documents from current schemas.
2. Complete WHERE/facet/count, grouping/live, waterfall and record-detail interactions, including exact visible trace rows and sensitive payload denial.
3. Prove states, normalized data, cancellation and host action/navigation behavior through T1–T5.
4. Inspect each component and each signal variant before dependent family/page use.

## Ordered Implementation Scenarios

### Scenario 1 — Canonical signal interactions

**Behavior.** Visible root traces load bounded span/error counts; flat spans remain distinct; same WHERE/range drives facets and lists. Waterfall/detail show exact data, with real log severity and duration_nano fields; live pauses and new-row notice preserve inspection.

**RED.** Add family state/loader/interaction tests; reusable family exports are absent. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-004: Canonical signal interactions$'
```

**GREEN.** Implement the family over foundation reads; never invent span names, metrics values or verification content. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

### Scenario 2 — Authorized Observe across hosts

**Behavior.** Every data component passes T1–T5, with empty/denied/error/loading, supported degraded results, cancellation and sensitive payload refusal; unrelated tenant rows never appear.

**RED.** Add focused family artifact and Node live journey assertions against seeded observations. Run this exact focused command (new Vitest title where applicable):

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^TASK-004: Authorized Observe across hosts$"'
```

**GREEN.** Reuse real observation registration/write/flush/read and bounded renderer behavior. Rerun this scenario and all earlier changed scenarios in this task. A missing prerequisite is a dependency, not permission to weaken the test.

**REFACTOR.** Simplify within the existing owner while these scenarios stay green. Private helpers, fixture structure and module layout remain implementation-owned; do not add an alternate transport, lifecycle or orchestration owner.

## Acceptance Criteria

- Each of StatStrip, FacetRail, RecordTable, TraceWaterfall and RecordDetail has complete T1–T5/state/interaction and individual visual evidence.
- Canonical metrics retain their recorded type; missing values are not zero; no histogram quantile is fabricated.
- Service-scoped variants complete only after actual REQ-023 contract and data are available.

## Expected Write Set and Consumer Closure

Package Observe components/loaders/catalog inputs, shared gallery/fixtures/tests; generated wire roots only as required. No Observe routes or new ingest schema. Paths guide owner discovery, not a private implementation allowlist. Close directly affected consumers, generated outputs, runtime package contents and tests in this task; do not defer a broken consumer to a later task.

## Verification and Evidence

Run the exact scenario commands above. [Verification setup](../references/verification.md) defines the current test owners and repository-managed prerequisites. New Vitest titles are planned additions to the existing runner, not claims of existing tests. Select actual running tests; skipped/no-selected cases are not proof. Do not add dependencies or environment settings to make selection convenient.

Run `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check` and `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build` after focused tests. Run `mise run codegen:check` when catalog/generated inputs change. Record T1 states, T2 actual requests/shape, T3 view validation/render, T4 actual built AppBridge HTML, T5 real authorized Node/server data and T6 only for assembled pages. Data-free primitives have no invented loader/live proof. Preserve per-component fixture provenance and interaction/action-host evidence.

### Human visual checkpoint

Main, Spans, TraceDrawer, Logs, Metrics, Agent and relevant Verify list uses; inspect each component separately. Run the app with its normal development command (`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui dev`) for gallery work, or the built production BFF/current native/container launch for deployed proof. Family tasks use `/_gallery`; page tasks use their authenticated route. Real journey data/credentials come from the repository-managed setup, never a production credential in fixtures. Use representative populated, empty, denied, error and loading states; desktop and mobile, light and dark, keyboard/focus, touch, long content and narrow overflow. Record acceptance separately for every owned component/page checkpoint. Wait for human inspection before starting the next dependent slice; family acceptance never waives an individual component. A human-uninspected slice remains unaccepted.

## Material Stop Conditions

Stop for a changed public contract, credential/security or durable ownership requirement outside the approved spec, or a missing external contract the task explicitly consumes. Finish independent in-scope work; report the precise dependency. Do not infer platform payloads from a canvas or mock. Current main conflicts require authority resolution; ordinary private design/fixture choices do not. Missing cross-platform or human proof remains unverified rather than being reported as a pass.

## Authority Links

- [Approved spec](../spec.md), [task ordering and obligation map](../plan.md), [source evidence](../references/evidence.md), [canvas](../references/canvas/).
- [AGENTS.md](../../../../AGENTS.md), [agent rules](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [doctrine](../../../../architecture/wyrd-design.md).
- [Execution](../../../../changes/implementation-execution.md), [TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md).
- [Bifrost contracts](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [deployment/release](../../../../architecture/operations/deployment-and-release.md).

## Implementation Evidence

Owners reused: `Bifrost`/`bifrost.query` through the TASK-003 executor and `loadPanel` (no second transport); canonical `bifrost.traces.spans`, `logs.records`, `metrics.points`, `gateway.calls` read directly. New code: `packages/ui/src/lib/observe.ts` (bound-clause SQL planner and shapers), five components, `SIGNALS` allowlists in `schemas.ts`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| StatStrip, FacetRail, RecordTable, TraceWaterfall, RecordDetail: T1 states | `packages/ui/src/lib/components/*.svelte`; `EmptyState` for loading/empty/denied/error | `TASK-004: Canonical signal interactions` (state shells for all five) proves states only; no comparison against the canvas boards was made. Board fidelity (Main, Spans, TraceDrawer, Logs, Metrics, Agent, Service) fails on layout, columns, typography, colour and formats | FAIL |
| T2 actual requests and shape | `observe.ts::Binds`, `listQuery`, `countsQuery`, `facetQuery`, `statQueries`, `waterfallQuery`, `detailQuery`; values bound as `$n`, columns from `SIGNALS` | Exact SQL/params for traces (root rows of traces with any matching span), counts for visible traces only, logs facets, cost per currency, tokens from `gateway.calls.usage`; refused list issues no dependent read | PASS |
| T3 view validation and render | `LOADERS`, `VIEWS` in `WyrdView.svelte`; catalog regenerated | Schema refusals (unknown column, injected trace id, `where` on gateway stats); WyrdView with all panels and the live interval | PASS |
| T4 built AppBridge HTML | `dist/wyrd-view.html` | `TASK-004: Authorized Observe across hosts` AppBridge RecordTable + TraceWaterfall | PASS |
| T5 real authorized Node/server data, both hosts | OTLP/HTTP JSON seeding per tenant | Same test over SDK and MCP: traces with counts `[TRACE,3,1]`, child-span WHERE lists its root, distinct trace facet, waterfall offsets 0/100/200 ns-exact, span/log detail, foreign tenant trace empty, `cards:read` principal denied on all five | PASS |
| Interactions: WHERE/facets/counts, grouping, live hold, waterfall, detail | `RecordTable` hold (hover/focus/scroll/open, rows and counts together, "↑ N new"), `groupBy`; `TraceWaterfall` collapse/expand/errors-only/selected path; `RecordDetail` tabs and intents only with a host | Unit interactions incl. held counts after open, refused counts read "—", deep `spanId` opens its path | PASS |
| Metrics keep recorded type; missing ≠ zero; no fabricated quantile | Metric columns pass through; structure columns (`bucket_counts`, `explicit_bounds`, …) listable; null cells render ∅, null stats "—" | T5 `ui.requests` sum int 7 / `ui.load` gauge double 0.5 with nulls; tokens `[[null]]`, cost `[]` with no gateway calls | PASS |
| Service-scoped variants on REQ-023 | `StatStrip.scope` binds `workload_card_uid`/`subject_card_uid` | T5: span tagged `wyrd.card_uid` = declared Model → subject scope counts 1 of 3; unrecorded uid empty | PASS (subject); workload path shares the same bind, not separately seeded |

Commands: the two scenario commands above (unit 1 passed; integration file 3 passed), `pnpm test` 42 passed, `pnpm check` 0 errors, `pnpm build`, `mise run codegen:check`, `git diff --check`.

Diagnoses:
- **Live WyrdView test timed out.** Symptom: `waitFor` never resolved under `vi.useFakeTimers`. Evidence: testing-library polls with the faked `setInterval`. Cause: the poll itself was frozen. Fix site: the test awaits a promise resolved when the counts request reaches the executor; no timeout change.
- **T5 span `kind` mismatch.** Symptom: expected `'Unspecified'`. Evidence: `SpansTable` stores `kind` as Int32. Cause: wrong expectation. Fix site: assertion uses the stored `1`.
- **Unlabeled DurationBar had zero-width track.** Cause: the grid kept a label column. Fix site: `DurationBar.svelte` `.unlabeled` grid.

Limits: Int64 values above 2^53 lose precision in JSON transport; spans have no column-level sensitive-payload denial, table permission governs every column; `FilterSchema` expresses equality, not-equal and null only; verification-result signal, duration-bucket facets and pagination are not in this task.

Non-goals confirmed: no Observe routes, no ingest schema, no new transport or lifecycle owner, no dependency added; only the UI package, gallery, fixtures and tests changed.

Human visual checkpoint: not reached. The components do not match their canvas boards and the gallery does not label each component with its board; the task is not IMPLEMENTED.

## Current closure — revision 18, 2026-11-02

The implementation and historical evidence above are retained. The caller now
requires a standard live development workflow and working shell/Traces/Spans/
Logs/Metrics before TASK-004 can complete. Current tasks:

1. [TASK-004-REV-01 — MockServer](TASK-004-revision/TASK-01-mockserver.md).
2. [TASK-004-REV-02 — Live Observe workflow](TASK-004-revision/TASK-02-live-observe-workflow.md).

All remaining R1 component findings, states, truthful signal behavior and visual
checkpoints transfer to Task 02 alongside the full TASK-012/020/021/022 scope.
The old [R1/R2/R3 records](../references/retired-task-004/) are retired historical
evidence. No old PASS claim or source edit establishes independent task review,
real-browser success or human acceptance. Task 02 depends on the existing
foundation/Observe implementation, not on this parent being complete: its
independent PASS and required individual live/component acceptance close this
parent. Preserve code and caller cleanup; do not restore rejected mocking.
