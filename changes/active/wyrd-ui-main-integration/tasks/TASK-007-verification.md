# TASK-007 — Verification components and investigation

Spec: [revision 28, approved](../spec.md). Status: Proposed; the requirement-detail prerequisite below prevents full completion from the current checkout.
Dependencies: completed TASK-003, TASK-004 live shell/Observe and TASK-027; [TASK-005 Cards and Prompts](TASK-005-cards-and-prompts.md) for exact Card routing and shared Card content.
Mapped obligations: REQ-018/020/022/023/024/026/027/030/034/062; AC-007/009/010/011/013/016/017/019/041; INV-001–INV-007 apply, with installation-only obligations retained by their existing owners.
Absorbs former TASK-007's VerifierGraph, VerificationResult, RequirementDetail and DriftFeatureTable obligations and all former TASK-023 outcomes. VerificationBindings belongs to TASK-008; AttentionList belongs to TASK-010. Those transfers do not remove their library obligations.

## Outcome and Value

A user finds a recorded failure in Verify, opens its selected judgment, follows
the requirement's evidence and inspects the exact Verifier definition. The same
four reusable components work in the gallery, a custom app and the MCP App
artifact while their conventional UI is delivered in this task.

An addressed execution that is still waiting shows progress without a verdict;
it never appears as a failed or passed recorded result merely because no result
has arrived.

## What Changes and What Must Stay True

- Own `VerifierGraph`, `VerificationResult`, `RequirementDetail` and
  `DriftFeatureTable`, their portable loaders, catalog/view entries and typed
  navigation intents. Reuse Chart, FacetRail, RecordTable, CardHeader,
  CardFacts, RelationshipList and exact trace inspection rather than duplicating
  them. Components render typed props; loaders sequence reads and shape rows;
  the existing Node/MCP executors and session BFF own authorized IO.
- Enable Verify in the tenant shell and deliver `/t/[tenantKey]/verify` with a
  shareable `?execution=...` selection and
  `/t/[tenantKey]/verify/executions/[executionId]` expanded detail. The list owns WHERE, time, Live and grouping
  state; opening/closing its drawer preserves filters and position. Expand opens
  a directly addressable result with the same exact execution identity. The
  exact Card route `/t/[tenantKey]/cards/detail?kind=Verifier&name=...&version=...`
  (with optional `uid`, safely encoded) established by TASK-005 renders the plain definition,
  YAML and where-it-runs, with **Results in Verify**, never a Results tab.
- The list contains recorded results only. It has two brushable charts (by
  recorded verdict and by Verifier); facets for verdict, Verifier, failed
  requirement, Workload, Card and session; columns Time, Verifier, Card,
  Workload, Run, Session, Failed requirements and Verdict; groups none, run,
  Card, Workload and session. Do not expose duration, hash, retry internals,
  provenance or table names in the investigation UI.
- Build the graph from exact `VerifierSpec` inputs, steps layered by output
  reads and `depends_on`, labeled dependency edges, step requirements and
  Verifier-level requirements. Color it only from the selected recorded
  judgment; select the first recorded failed requirement. Requirement detail
  shows reason, actual, citations and judge; Inputs starts collapsed.
- Distinguish `execution_id`, selected `result_id`, application `run_id`,
  subject, exact Verifier and binding workload. Session groups are scoped to
  tenant and exact workload owner. Read the selected execution's
  `GroupSnapshot.sources[*].execution_ids` for pinned membership and preserve
  separate later reviews; matching caller `group_id` text is not membership.
- Drift shows every recorded feature, drifted first, score/threshold/verdict,
  recorded score history with threshold and previous/next windows. Never
  derive requirement/aggregate verdicts from scores or draw an unrecorded
  baseline distribution.
- Trace citations read the exact trace/span under current viewer permissions,
  including cross-workload, untagged and out-of-originating-range spans. Do not
  propagate the result list's predicates into that evidence read. Preserve the
  list on return, expose denied/truncated reads and keep late spans from
  changing a completed judgment. Server-owned evidence retries remain server
  behavior; no new execution-list API, client resubmission or retry controller.

**Requirement-detail prerequisite.** Current `bifrost.verification.results` is
payload-free. Requirements, steps and drift feature detail live in sensitive
`bifrost.verification.execution_outputs.output`; `bifrost.drift.result_features`
does not exist. This task does not reclassify that payload or grant elevated
access to ordinary viewers. Before implementing the affected projection, require
the separately approved non-sensitive read contract, its generated types,
SDK/MCP projections and authorization journeys. Record its exact source and
approval in the task evidence. Until then, independent definition/summary work
may proceed, but requirement detail, failed-requirement facets, colored result
graphs and per-feature live completion remain blocked. Do not guess the missing
payload from a canvas or label a fixture/unavailable control production-complete.

## Approved Canvases

These approved canvases define the visual target for this task. Follow their layout, component placement, hierarchy and interactions while using current Wyrd contracts and brand tokens. Apply the same component design in the gallery and the working UI. Record a separate comparison and human acceptance for each named canvas on desktop/mobile in light/dark.

- [Verify.dc.html](../references/canvas/Verify.dc.html)
- [VerifySession.dc.html](../references/canvas/VerifySession.dc.html)
- [VerifyDrift.dc.html](../references/canvas/VerifyDrift.dc.html)
- [Verifier.dc.html](../references/canvas/Verifier.dc.html)

If a required visual detail is absent or contradictory in these approved canvases, stop that affected visual work and ask the maintainer for clarification. Do not guess a layout, invent a replacement design, or treat a generic directory link as a visual specification.

## Approach

1. Confirm current execution/result/security contracts and prerequisite delivery;
   preserve the already-correct session and exact trace owners.
2. Extend the shared package with definition and authorized result projections,
   preserving exact identities, selected results and read cancellation.
3. Add catalog, gallery and built-artifact coverage alongside each component.
4. Mount the same components on Verify and Verifier routes, closing navigation,
   filters, drawers and citation actions through existing hosts.
5. Extend normal development setup through public SDK workflows so `dev:full`
   can demonstrate real recorded run, group and drift states, then prove and
   inspect each component and page separately.

## Ordered Implementation Scenarios

### Scenario 1 — Exact definition and selected recorded judgments

**Behavior.** An exact Verifier Card renders a plain graph from its declaration.
An addressed completed execution resolves its selected result and exact Verifier
before coloring that graph. A pending/running/retrying execution, or a completed
execution whose selected result is not visible yet, shows waiting without an
invented verdict. Terminal errors remain errors. Two reviews with the same
caller group text but different owners or snapshots do not share members.

**RED.** Add planned `verification definitions preserve selected judgments`
under the existing library renderer runner ([V1](#v1--definition-and-result-projection)).
Assert declared dependency edges, plain versus colored modes, exact identity,
selected-result precedence over abandoned rows and pinned source execution
membership. The missing components/projections fail these assertions. A denied
prerequisite must issue no dependent read.

**GREEN.** Implement these projections in portable loaders over current reads
and the approved requirement-detail prerequisite where needed. Rerun V1 after
each correction; missing authorization is never an empty successful judgment.

**REFACTOR.** Reuse the existing load/state/action owners while V1 stays green;
private layout and helper choices remain implementation-owned.

### Scenario 2 — Results, requirements and drift are inspectable across hosts

**Behavior.** The list, charts and facets apply the same filters/range. Opening
a run/session result shows its exact graph and requirement detail; opening drift
shows all recorded features, their recorded values and selected window. Empty,
loading, denied and stable error states remain distinct. Rapid input/range/window
changes cancel supported work and superseded responses never replace new state.

**RED.** Add planned `verification components preserve recorded states`
([V2](#v2--component-states-and-host-rendering)) and `authorized verification
investigation across hosts` ([V3](#v3--real-authorized-data)). Assert every owned
component's prop states, actual planned reads/binds, view-document validation,
the actual built AppBridge HTML, and real Node/MCP parity with populated, empty,
underprivileged and foreign-tenant cases. Invalid catalog input fails before IO.
Do not manufacture a RED for a projection whose prerequisite is still absent.

**GREEN.** Complete the component/catalog/loader/host behavior together. Use
public verification workflows for real results, not writes to protected result
tables. Rerun V1–V3 and retain explicit blocked evidence for unavailable contracts.

**REFACTOR.** Keep one recorded-result interpretation shared by hosts; remove
duplicate page shaping without weakening any state or tenant assertion.

### Scenario 3 — Citations preserve the judgment and evidence boundaries

**Behavior.** A citation opens exact authorized evidence beyond the list's
range/workload/subject predicates, then returns to the preserved list. Direct
missing evidence and exhausted queued evidence show the server-recorded
inconclusive requirement reason; other requirements and aggregate verdict remain
recorded. Later retained spans enrich inspection, never recompute that judgment.
Denied and bounded/truncated evidence stay visibly incomplete.

**RED.** Extend planned `authorized verification investigation across hosts`
([V3](#v3--real-authorized-data)) with queued-arrival, direct-absence,
late-arrival, cross-workload/untagged citation and denied/truncated evidence cases.
Assert selected result identity before and after late evidence and exact trace
reads without originating list predicates. This fails if a loader invents
membership, retries a judgment or treats an incomplete trace as complete.

**GREEN.** Reuse execution readback and existing exact Observe navigation; keep
read failures local to the affected panel. Rerun V1–V3, including cancellation
and denial of a prerequisite without follow-on reads.

**REFACTOR.** Consolidate only repeated host navigation/projection work, keeping
server execution ownership and the original list state intact.

### Scenario 4 — Normal development exposes the complete investigation

**Behavior.** After `mise run dev:full`, a normal local human signs in, reaches
Verify from the shell, filters real results, opens run/session/drift detail,
changes range/window, follows a citation and opens the exact Verifier Card.
The gallery contains every new component at the same delivery checkpoint.

**RED.** Add planned `authenticated verification investigation`
([V4](#v4--real-browser-journey)). Extend the existing public-signup/password-login
and production BFF/browser support, asserting visible results and actual
authorized server reads for these actions. Include denied/error/empty and
cross-tenant cases; imported handlers, cookie simulation or fixture page props
cannot satisfy this test. Check normal setup actually supplies representative
data rather than accepting a route that opens with no useful records.

**GREEN.** Mount shared components and expose the existing typed navigation
intents, then extend the existing dev setup through public workflows only where
the canonical workload lacks required examples. Record seeded identities,
expected/actual recorded outcomes and routes; `dev:batch` must keep working.
Rerun V1–V4 and the scoped checks below.

**REFACTOR.** Keep conventional pages responsible only for layout/URL/control
state. Reuse the existing development lifecycle and browser harness.

## Acceptance Criteria

| Obligations | Required observation and proof |
|---|---|
| REQ-018/026/027; AC-009/019 | V1–V3 retain exact selected judgments, owner-scoped pinned members, waiting/error states, declared graphs and recorded per-feature outcomes. An abandoned row, later span or client-computed verdict changing the judgment fails acceptance. |
| REQ-022/023/024; AC-007/016/017 | Actual reads use canonical current fields and exact workload/subject/Verifier identities; null attribution stays null. Matching component/account/group labels grant no association or permission. |
| REQ-030; AC-011; INV-001–007 | V2/V3 cover T1 props, T2 actual requests/shapes, T3 validated views, T4 built AppBridge artifact and T5 real authorized data for each applicable component. Neither sensitive-payload bypass nor fixture-only completion is accepted. |
| REQ-034/062; AC-013/041 | V4 and the normal `dev:full` checkpoint prove enabled routes, human login, populated useful workflows, preserved filters/range/selection, failures and tenant denial. Each new component has its gallery and first live consumer here. |
| REQ-020; AC-010 | Individually accept VerifierGraph, VerificationResult, RequirementDetail and DriftFeatureTable, then Verify, VerifySession, VerifyDrift and Verifier boards on desktop/mobile in light/dark. Record reference region, screenshot/state/viewport, keyboard/focus, touch, long content/overflow and explicit human acceptance; grouped task acceptance waives none. |

## Implementation Reference

Source locations guide owner discovery, not a private file/type allowlist.

- **Existing execution owner:** `sdks/wyrd-sdk-ts/wyrd/src/index.ts::verify.Execution.status`
  and `crates/shared/wyrd-client/src/verify/execution.rs::Execution::status`
  supply current `VerificationExecutionState`, selected result and `GroupSnapshot`.
  Inspected consumer: `sdks/wyrd-sdk-ts/wyrd/tests/integration/verify-in-real-time.test.ts`
  uses public execution/group workflows. Extend UI projection, not execution
  lifecycle or an aggregate execution-list API.
- **Current storage boundary:** `ResultsTable::arrow_fields` in
  `crates/vala/vala-bifrost-redux/src/tables/verification/results.rs` supplies
  summary identities/verdict/windows; `ExecutionOutputsTable` in the adjacent
  `execution_outputs.rs` explicitly classifies `output` as sensitive. The drift
  module documents per-feature detail there. No current result-features table
  or public safe-detail contract is assumed by this task.
- **Existing portable owner:** `packages/ui/src/lib/loaders.ts::loadPanel`,
  `contract.ts::{WireRequest,Loaded,ActionIntent,ActionState}`, `node.ts::nodeExecutor`
  and `mcp.ts::mcpExecutor` already support exact Cards, execution reads and
  bounded queries. Extend schemas/catalog/component exports and these projections
  only as current approved public contracts permit; add generated type roots
  rather than handwritten Verifier/outcome domain types.
- **Existing conventional consumer:** `src/routes/t/[tenantKey]/observe/+page.server.ts::load`
  and `src/routes/t/[tenantKey]/ui/load/+server.ts::POST` call `loadPanel` with
  `nodeExecutor(session.client())`. Preserve their credential/tenant boundary;
  new Verify route/layout state and navigation reuse this pattern.
- **Inspected proof owners:** `src/lib/ui-library.test.ts`,
  `ui-library.integration.test.ts::{appBridgeView,mcpOver}`, and
  `ui-pages.integration.test.ts::{startBff,humanSignIn}` already cover package
  rendering, real SDK/MCP reads and Chromium against the production BFF.
  `scripts/dev/example-app.sh` and `mise.toml` own normal seeding/batching.
  Extend them for missing verification examples; do not add another launcher,
  fixture production fallback or query service.

## Verification and Evidence

All V1–V4 selectors below are **planned**, not claims of existing passing tests.
After adding each test, confirm its actual selector before RED and record selected
executed cases; skipped/no-selected cases are not proof. `package.json` owns
`test` (`vitest run`); `vite.config.ts` includes these files and enables real
integration only with existing `WYRD_UI_INTEGRATION`.

### V1 — Definition and result projection

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^verification definitions preserve selected judgments$'
```

### V2 — Component states and host rendering

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^verification components preserve recorded states$'
```

### V3 — Real authorized data

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "^authorized verification investigation across hosts$"'
```

### V4 — Real browser journey

```bash
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-pages.integration.test.ts -t "^authenticated verification investigation$"'
```

Before V3/V4, use the current [managed setup](../references/verification.md):
`mise run ts:build`, `mise run ts:build:testing`, `mise run build:server`, the
existing workspace installation and test browser installation, and current
production UI/library/artifact builds. No new runner, selector environment or
production switch is authorized. Run these scoped checks after focused tests:

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run codegen:check
```

For manual acceptance run `mise run dev:full`; use its printed login, inspect
Verify and the exact Verifier Card, and inspect `/_gallery`. Record actual
seeded cases and any missing prerequisite separately from rendered state proof.
Retain source/browser identity, sanitized requests/screenshots/traces, fixture
provenance, action success/denial/error and each human checkpoint. Production
MCP registration/live conversation remains TASK-012's final obligation; T4 and
component-specific host parity do not wait for it.

## Material Stop Conditions

Stop the affected work for an absent approved safe-detail contract, a changed
security/public/persisted interface, or inability to seed the approved behavior
through existing public workflows without a new platform capability. Complete
independent scope and name the exact blocked projection. Do not reclassify
sensitive output, invent safe payloads or grant new access. Private component
layout, test fixture structure and helper placement are implementation decisions.
Missing platform/browser/human proof remains unverified, never PASS.

## Authority Links

[Approved spec](../spec.md), [delivery and obligation map](../plan.md),
[approved canvases](#approved-canvases), [reference audit](../references/task-audit-2026-10-29.md),
[AGENTS.md](../../../../AGENTS.md), [execution](../../../../changes/implementation-execution.md),
[TDD](../../../../changes/spec-driven-development.md), [testing](../../../../TESTING.md),
[Wyrd design](../../../../architecture/wyrd-design.md),
[security](../../../../architecture/wyrd-security-posture.md).
