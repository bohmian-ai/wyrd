# TASK-010 — Service workspace and versioned charts

Spec: [revision 28, approved](../spec.md). Status: Proposed; chart persistence and Change Request regions are **externally gated**.
Dependencies: completed TASK-003/TASK-004/TASK-027; [Cards and Prompts](TASK-005-cards-and-prompts.md), [Query](TASK-006-query-workbench.md), [Verification](TASK-007-verification.md), [Card workspaces](TASK-008-card-workspaces.md), [Changes and Repos](TASK-009-changes-and-repositories.md) for its Change Request region.
Mapped obligations: REQ-013/020/022/023/024/025/029/030/034/062; AC-007/010/011/013/017/041; INV-001–INV-007.

External activation dependencies: separately approved and shipped `ServiceSpec.charts` registration/validation/read/action contracts with generated SDK/MCP projections, plus the delivered Changes contract for Service CR links and attention rows. These are platform prerequisites, not backend implementation assignments here. Independent signal/summary work can proceed against delivered sources; unavailable Save actions, fixture-only charts or omitted CR sections cannot close the complete task.

## Outcome and Value

An exact Service release has a working Overview and Charts workspace built from reusable library components. The user sees its composition, actual isolated signals, attention, bound Verifiers, related Change Requests, versions and YAML, then previews or saves a chart as a new Service version. Normal `mise run dev:full` exposes representative data and the same mounted components the gallery validates.

For example, two Services can use the same Agent while their Service pages retain separate traffic and verification results. Saving a chart never changes the selected release in place: the server registers a new exact version, which the user can inspect and compare with the original.

## What Changes and What Must Stay True

Own `SignalBreakdownTable`, `ChartEditor` and `AttentionList`, including typed props, portable loaders, catalog/view support, gallery states, action intents and their first live consumers. Extend the shared exact Card detail route from TASK-005: `/t/[tenantKey]/cards/detail?kind=Service&name=...&version=...`, with optional `uid`, built through the same encoded navigation owner. Overview, Charts, Versions and YAML are reachable, shareable sections on that exact identity. Reuse CardHeader, CompositionGraph, VerificationBindings, StatStrip, Chart, CardFacts, RelationshipList and ChangeRequestList; do not duplicate their owners.

### Scope and recorded facts

- Overview covers deployment facts, composition, Signals, By component, attention, bound Verifiers, Change Requests and relationships/metadata. Charts covers list, preview and editing. Versions and YAML retain the exact Card contract. Do not expand into deployment management.
- Exact release signals filter by the selected Card's `workload_card_uid`; By component groups by `subject_card_uid`. Never infer membership from component UID lists, resource labels or shared Agent membership. Preserve null/unknown attribution rather than reassign it. A workload UID is not a Run/trace/execution join key. If an existing family-wide scope is consumed, resolve its registered exact version UIDs through Cards, not name matching; no new family selector is required.
- SignalBreakdownTable uses recorded root spans and gateway calls through existing schema descriptions and query owners. Preserve typed values, times and exact decimal usage/cost; do not add currencies or unlike usage dimensions as one total. Every chart's Open in Query preserves actual SQL, typed binds, range and bounds. Absent telemetry names its producing attribute/binding; it is not fabricated zero or an invented Service-request span.
- AttentionList projects recorded failed verification and approved Change Request attention under the requested workload/range. Its verdicts come from recorded results; an addressed waiting execution is not a failed result. Exact links open the original result/CR, not latest versions. No access to sensitive requirement outputs is inferred from a summary grant.

### Chart authoring and action semantics

Consume the shipped generated chart contract with `id`, `title`, `type` (`timeseries`, `bar`, `stat`, `table`), exactly one of inline `sql` or registration-resolved `sql_file`, `x`, `y`, optional `series` and `unit`. Do not add these fields to handwritten frontend domain types or implement registration validation here. Preview uses the existing read-only query boundary. Range start/end are RFC 3339 strings bound as `$1`/`$2`, bucket is `$3`, with SQL casts as required by the supported QueryParam contract; timeseries SQL bounds its time column by the range. Preserve authored SQL verbatim and reject incompatible result shape visibly.

| Chart type | Required preview/render behavior |
|---|---|
| timeseries | Timestamp x, numeric y column(s), optional text series; ordered x, range bounded; more than twenty series fold into recorded aggregate “other” according to the approved chart contract. |
| bar | One text category and numeric values. |
| stat | Exactly one row and one numeric column. |
| table | Any supported result shape, capped with visible truncation/bounds. |

ChartEditor emits a validated JSON-safe intent. The host authorizes registration as the current viewer and registers a new exact Service version, preserving unrelated spec content. A successful response/navigation identifies that new version and leaves the original unchanged. Pending submissions cannot duplicate writes. Denial/schema error/upstream failure remains visible with a safe draft; no fabricated success or automatic retry of an ambiguous registration. Server-derived relationships/status remain server-owned.

Components render props; shared loaders own SQL/read planning and shaping; Node/MCP/BFF hosts own credentials and IO; pages own layout/navigation state only. A range/version/subject change supersedes prior requests, cancels where supported and cannot be overwritten by late results. A failed prerequisite issues no dependent queries or writes. Query/panel failure stays local and recoverable; it must not stop the shared server/BFF or clear unrelated loaded content. No new graph, telemetry store, chart persistence service, query runner, registration engine, Ask runtime or fixture fallback.

### Visual coverage

Service Overview: deployment facts, composition, Signals, By component, attention, bindings, CRs, relationships/metadata. ServiceCharts: chart list/preview/Edit chart and action states. Record separate Overview and Charts acceptance, plus individual SignalBreakdownTable, ChartEditor and AttentionList checks. Their Home placements remain the Home task's page acceptance; this task establishes the reusable components through the first real Service consumer.

## Approved Canvases

These approved canvases define the visual target for this task. Follow their layout, component placement, hierarchy and interactions while using current Wyrd contracts and brand tokens. Apply the same component design in the gallery and the working UI. Record a separate comparison and human acceptance for each named canvas on desktop/mobile in light/dark.

- [Service.dc.html](../references/canvas/Service.dc.html)
- [ServiceCharts.dc.html](../references/canvas/ServiceCharts.dc.html)

If a required visual detail is absent or contradictory in these approved canvases, stop that affected visual work and ask the maintainer for clarification. Do not guess a layout, invent a replacement design, or treat a generic directory link as a visual specification.

## Approach

1. Confirm delivered chart/CR prerequisites and exact read/action/schema projections; keep unactivated work explicit.
2. Add workload-isolated signal and attention components/loaders alongside real Overview mounting.
3. Deliver ChartEditor and Charts section using existing Query/Chart consumers and actual versioned registration.
4. Extend the existing development setup with two Services sharing one component and representative verification/CR/chart content through public workflows.
5. Prove shared host behavior, deployed browser interactions, normal startup data and individual gallery/page checkpoints.

## Ordered Implementation Scenarios

### Scenario 1 — Exact Service signals and attention stay isolated

**Behavior.** Given two Service versions sharing an Agent, selecting either release shows only its recorded workload signals, per-subject breakdown, verification bindings/attention and CRs under the requested range. Missing subject/workload fields remain missing; denied/error never becomes empty success. Recorded failed rows link to exact evidence; queued/waiting execution states do not manufacture failures. All three new components expose their applicable populated/empty/denied/error/loading states, typed intents and catalog validation; range/version supersession prevents stale overwrite.

**RED.** Add the planned [isolation component test](#v1-service-isolation). Assert actual SQL/binds/identities, dependent-read denial, state shapes and intent payloads against generated-contract fixtures, not component existence. It fails because shared breakdown/attention projections and Service consumers are absent.

**GREEN.** Implement portable projection in existing library owners and mount the same components on exact Service Overview. Reuse existing graph/bindings/stats; rerun this scenario.

**REFACTOR.** Share existing scope/query/formatting helpers only where they preserve source precision, shape and authority; no cross-Service membership cache or frontend verdict calculation.

### Scenario 2 — Chart preview validates actual SQL and result shapes

**Behavior.** Editing inline/file-backed chart definitions previews all four chart kinds with exact typed binds and current read permissions. Ordered/bounded timeseries, multi-series folding, numeric/schema mismatches, empty/degraded/capped results and upstream/permission errors are visible. Malformed inputs never execute SQL. Open in Query reproduces exact SQL/params/range without changing authored SQL. ChartEditor states are independently renderable in `WyrdView` and the gallery.

**RED.** Add the planned [preview test](#v2-chart-preview). Exercise the shape table and assert exact planned requests and visible stable errors, series overflow behavior, result bounds and editor intents.

**GREEN.** Extend existing Chart/Query schemas and consumers only where the activated chart contract requires them; do not build another query interpreter or validator. Rerun Scenarios 1–2.

**REFACTOR.** Reuse loadChart/shapeChart/chartParams and shared Query presentation with no hidden chart-specific transport.

### Scenario 3 — Real hosts register a new chart version as the viewer

**Behavior.** Genuine Node and MCP consumers load the same exact Service data and charts; the built AppBridge HTML renders SignalBreakdownTable, AttentionList and ChartEditor. Each supported action host saves through the actual authorized registration projection. Success returns a new Service version containing the chart and unchanged unrelated content; old version readback is unchanged. Duplicate pending submissions, denied writes, schema refusal and uncertain failures cannot report false success or silently retry. Cross-tenant reads/actions expose no data.

**RED.** Add the planned [authorized host journey](#v3-authorized-service-hosts) after the external projections ship. Seed via public Card/verification/observation/CR workflows, not protected table writes. Assert original/new versions, real caller permission errors and per-component T4/T5 evidence.

**GREEN.** Wire activated actions through existing host intent validation/state completion and current registration owners; extend generated contract consumers rather than creating frontend persistence. Rerun Scenarios 1–3.

**REFACTOR.** Remove duplicated projection/action handling while preserving read-only executor boundaries, caller auth, stable refusal and no unsafe retries.

### Scenario 4 — Real browser inspects and edits an exact Service release

**Behavior.** A browser signs in through normal human auth, enters Cards, opens the exact Service and navigates Overview/Charts/Versions/YAML. Inspect composition links, bindings, attention and CR detail. Change range and version, follow Open in Query, preview/edit/save a chart and verify the new version in the registry. Preserve prior URL/filter state on return. Cover empty/no-telemetry, denied read/write, schema and upstream errors, foreign-tenant identity, and delayed obsolete loads. Components display actual data through production BFF/server; the shell remains usable after panel failure.

**RED.** Add the planned [Service browser journey](#v4-service-browser), extending current production BFF/Chromium/public-login support. Missing Service sections or a fixture-only registration must fail observable browser/server assertions.

**GREEN.** Mount the same package components and shared loaders in the existing Card route, with host-only layout, navigation and action dispatch. Rerun all earlier changed scenarios and this journey.

**REFACTOR.** Share existing exact Card navigation and session consumers; no page-level SQL/joins, alternate auth or deployment controller.

### Scenario 5 — Normal development startup contains representative Service content

**Behavior.** Fresh `mise run dev:full` startup supplies two exact Services sharing a component, their isolated recorded signals and verification/attention, a real associated Change Request and at least one persisted chart through public setup. The user signs in normally, opens the documented Service sample from Cards, inspects Overview/Charts and saves a safe chart revision with real readback. Setup failures remain explicit; a responding but empty route or hand-seeded database is insufficient.

**RED.** Extend existing devFull process/browser proof with the planned [development Service journey](#v5-development-workflow). Assert setup data identities and visible content on normal startup, then inspect a new saved version; do not substitute another test server or testing-enabled application wheel.

**GREEN.** Extend current public dev setup once for missing representative data, preserving canonical workload, normal auth, fresh-database behavior and shutdown/diagnostic ownership. Rerun affected earlier scenarios and the development check.

**REFACTOR.** Reuse seeded Cards/results and existing setup functions where sufficient; do not create a second launcher, production fixture toggle or private write path.

## Acceptance Criteria

| Obligations | Observable acceptance and proof |
|---|---|
| REQ-013/023/024; AC-007/017; INV-001/004/007 | Exact Overview/Charts/Versions/YAML and all required regions work; V1/V3/V4 prove workload isolation, subject breakdown, truthful absence/precision and exact evidence navigation with shared components. |
| REQ-025; INV-006 | V2–V4 prove all shapes, SQL/bind preservation, visible errors, same-viewer registration of a new version and unchanged old version. Actual platform schema/action delivery is mandatory; fixture Save cannot pass. |
| REQ-029 | Service CR section and CR attention use real delivered server contracts through TASK-009 components/projections. V3/V4 exercise them; silently omitted or fabricated rows fail acceptance. |
| REQ-030; AC-011; INV-002/005 | SignalBreakdownTable, AttentionList and ChartEditor each have T1 states, T2 requests/shaping/supersession, T3 catalog/view, T4 actual built AppBridge and T5 authorized Node/MCP data/action evidence under V1–V3. No credential or foreign content leak. |
| REQ-034; AC-013 | V4 uses genuine browser auth and production BFF/server reads/actions for range/version/links/preview/save and negative cases; handler imports/jsdom do not substitute. |
| REQ-062; AC-041 | V5 plus human `dev:full` checkpoint provides documented populated Service samples and real chart save without hidden setup. |
| REQ-020/022; AC-010; INV-003 | Every new component and Overview/Charts page checkpoint has its own canvas/source/screenshot/theme/viewport/accessibility and human acceptance evidence. No invented Service signals or client judgments. |

## Implementation Reference

Repository-relative UI paths below are rooted at `crates/wyrd/wyrd-server/wyrd-ui/`; other paths are repository-rooted. They identify current owners, not a private-file allowlist.

| Existing owner and inspected consumer/test | Missing behavior and selected extension |
|---|---|
| `packages/ui/src/lib/observe.ts::statQueries/loadStatStrip`, `loaders.ts::loadChart/shapeChart/chartParams/loadPanel`, `node.ts::nodeExecutor`, `mcp.ts::mcpExecutor`; consumed by `src/routes/t/[tenantKey]/observe/+page.server.ts` and current `ui-library.test.ts`/`ui-library.integration.test.ts`. | Extend portable Service projections over current schemas/typed Bifrost queries. Reuse precise existing signal/Chart paths and cancellation, never a new telemetry transport or page query pipeline. |
| `crates/wyrd-spec/src/card/service.rs::ServiceSpec` owns components/verified_by/tables and currently has no charts. `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Cards.get/list` and existing registration/declaration workflow own exact Card content. | Chart activation requires shipped Service schema, registration validation, generated roots and Node/MCP action projections. Record exact delivered symbols before live chart work. This task consumes the platform implementation rather than adding persistence/validation. |
| `crates/vala/vala-bifrost-redux/src/tables/gateway/calls.rs::CallsTable`, `.../verification/results.rs::ResultsTable` and current trace table schemas own managed workload/subject attribution and recorded summaries. `observe.ts::STAT_SOURCES/statQueries` already reads usage/cost/counts. | Add component breakdown/attention projections keyed to exact workload and recorded result identities. Requirement detail remains subject to its separate sensitive-output contract; no new result_features table or inferred verdict. |
| TASK-005 exact Card detail/navigation; TASK-008 CompositionGraph/VerificationBindings; TASK-007 result/workspace links; TASK-009 ChangeRequestList and activated CR reads. Existing `BrowserSession.client`, BFF `ui/load::POST`, `loadPanel/validateIntent` own auth/input boundary. | Add Service sections and validated action dispatch to that same consumer, preserve exact versions, and close generated/catalog/custom-app/MCP built resource consumers together. |
| `src/lib/ui-pages.integration.test.ts::startBff/humanSignIn/devFull` and `ui-library.integration.test.ts::appBridgeView/mcpOver`; `mise.toml::dev:full`, `scripts/dev/example-app.sh::setup`. | Reuse real browser/Node/MCP lifecycle and public development setup. Current canonical startup does not establish two-Service/chart/CR coverage; add representative supported data and assert it without another runner or production switch. |

Expected closure includes package components/loaders/catalog/actions/exports, fixture provenance and gallery, generated consumers of activated contracts, existing Node/MCP/BFF host adapters, Card detail sections, current dev setup, focused tests and built resource package contents. Backend chart schema/validation and CR implementation remain separately owned prerequisites.

## Verification and Evidence

All named tests below are **planned** additions to existing UI runner files. Confirm actual selectors after adding them and before RED; zero-selected/skipped cases are not proof. `package.json::test` builds package outputs and runs Vitest; current `vite.config.ts` gates integration files with `WYRD_UI_INTEGRATION`. Reuse `appBridgeView/mcpOver` for T4/T5, `serverTest/startBff/humanSignIn` for deployed T6 and `devFull` for actual development startup. Setup uses installed workspace dependencies, `mise run ts:build`, `mise run ts:build:testing`, `mise run build:server`, current UI output and existing Playwright Chromium installation. See [verification owners](../references/verification.md); no new fixture settings or selector framework.

### V1 Service isolation

T1–T3 workload/range/subject projections, attention identities, error/denial and stale-load protection.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^service signals and attention preserve exact workload scope$'
```

### V2 Chart preview

T1–T3 all chart types, exact SQL/binds, invalid shape, bounds, authored SQL and intents.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^service chart preview validates recorded result shapes$'
```

### V3 Authorized Service hosts

T4 actual built AppBridge and T5 Node/MCP source/action closure, old/new Service version readback, denied/error/tenant cases. Requires shipped external contracts.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^authorized service charts and summaries across hosts$"'
```

### V4 Service browser

T6 real-auth production browser Overview/Charts/Versions/YAML/navigation and versioned save with negative flows.

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated exact service workspace and chart save$"'
```

### V5 Development workflow

Normal launcher owns its fresh database and services; isolate local config using the existing devFull support, without wrapping a second test-Postgres lifecycle. Assert real setup completion and populated sample identities/content.

```bash
mise exec -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t '^development service workspace and versioned charts$'
```

After focused scenarios run:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run codegen:check
```

Codegen proof applies when catalog/generated inputs change. Record TDD outcomes, actual selected cases, package/built-artifact identity, sanitized real requests/results, stable refusals and generated/runtime consumer closure. Keep Rust/SDK prerequisite checks with their approved platform owner; do not claim those tests were performed here.

### Human visual checkpoint

Run `mise run dev:full`, sign in normally at the printed tenant login, inspect `/_gallery`, then open the documented exact Service sample through Cards. Inspect each of SignalBreakdownTable, AttentionList and ChartEditor individually, then Overview and Charts separately, including version switch, failed save, empty and denied signals. Check desktop/mobile, light/dark, keyboard/focus, touch, long content and overflow. Record board region, source identity, screenshot, state, theme, viewport and explicit human acceptance; fixture states supplement live populated work. Do not begin the next dependent slice before its individual checkpoints are accepted. Missing platform or human evidence remains incomplete.

## Material Stop Conditions

Stop chart/CR-dependent implementation for missing approved/generated contracts, registration validation, supported host action projections or real proof setup. Record exactly what is missing and finish independent in-scope Service signals without calling the complete task passed. Escalate changed public/persisted/security contracts, new durable ownership, incompatible required shape/series semantics, unsupported safe development seeding or newly required sensitive reads; do not widen permissions or invent payloads. Private component layout, helper choice and fixture organization remain implementation-owned. No silent prerequisite omission or fixture production fallback.

## Authority Links

[Approved spec](../spec.md), [plan and ownership map](../plan.md), [approved canvases](#approved-canvases), [visual/source audit](../references/task-audit-2026-10-29.md), [AGENTS](../../../../AGENTS.md), [design](../../../../architecture/wyrd-design.md), [Bifrost](../../../../architecture/bifrost-design.md), [security](../../../../architecture/wyrd-security-posture.md), [execution](../../../../changes/implementation-execution.md), [spec-driven development](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md) and the repository `wyrd-ui` skill govern this task. Current generated contracts win over historical layouts.
