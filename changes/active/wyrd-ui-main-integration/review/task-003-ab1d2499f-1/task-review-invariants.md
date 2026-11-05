# TASK-003 independent invariant review

Result: **FAIL**.

Subject: repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, base `695ca914df61a8683059f6f32279ec2148424e06`, candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`; approved spec revision 12; original `changes/active/wyrd-ui-main-integration/tasks/TASK-003-shared-ui-foundation.md`. The cumulative range and commit history were inspected; unrelated merged main changes and the four expressly excluded commits are not findings. TASK-027 is an approved successor, not TASK-003 scope drift. No implementation changes were made, and no peer reports were consulted.

## Authority, navigation, and ownership

Applied supplied AGENTS.md, architecture/agent-rules.md, reference router, spec-driven-development, maintainer-style, testing-workflows and TypeScript guidance, UI skill, REQ-025/030 and their architecture anchors (shared client, server-owned identity and query semantics, session tenancy). No `.codegraph` index was present. Navigation expanded independently from the shared map through loaders → schema → component geometry/actions → WyrdView generation ownership → Node/MCP executors → existing SDK/query owner, BFF session loader, artifact bootstrap, package/codegen/build and examples.

Existing alternatives inspected: shared `JsonResultBuilder` already owns positional bounded result normalization and is correctly reused by server collect and native SDK; SDK Cards and Bifrost already own reads and are wrapped rather than replaced; `Bifrost::query_only`/its shared private constructor already represent HTTP-only clients, so the browser-session branch reuses that shape. `plot.ts::columnIndex` already owns first-occurrence column selection but `shapeChart` independently builds a conflicting lookup. `ActionButton` already owns visible action-state feedback but CardHeader's native version/tab controls bypass equivalent handling. New portable loaders, schema/catalog and renderer are expressly required and do not duplicate an existing full UI workflow. No new backend orchestration owner is needed for the corrections below.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Browser-safe package exports and production package contents | packages/ui package.json; type-only SDK imports in contract/schemas; node.ts isolated | Recorded check/build and bundle import; source inspection | PASS, darwin bundle only |
| Generated catalog is the input/action authority | schemas.ts; scripts/catalog.ts; validatePanel/validateIntent; catalog.json | Recorded codegen and scenario 1; source tracing | PASS |
| Chart timeseries/bar/stat/table faithfully render supported numeric shapes | shapeChart; plot.ts; Chart.svelte | Scenario 1 covers positive values and table duplicate names; signed and duplicate-binding gaps below | FAIL |
| REQ-025 series cardinality (>20 folds) | plot.ts::MAX_SERIES/timeSeries | Existing test explicitly pins 5 series → 4; static and read-only runtime reproduction | FAIL |
| Exact SQL and binds, panel range override and inherited common range | loadChart/chartParams; WyrdView::start | Common range covered; explicit panel override contradicted by runtime reproduction | FAIL |
| Denied prerequisites issue no dependent read; paging failures stay separate from Card result | loadCardHeader with resolved Card identity, bounded cursor loop and signal check before next page | Scenario 3 and recorded live CardHeader proofs; source tracing | PASS |
| Exact Card reference, version selection, clipboard and host actions | CardHeader; validateIntent; WyrdView::act | Version emit/copy and Observe action covered, but version/tab action state discarded | FAIL |
| Superseded results cannot overwrite current view; dependent reads stop | WyrdView controllers, settle guard, effect cleanup; loadCardHeader signal | Scenario 3 replaces inputs and observes aborted signals | PASS |
| Cancellation propagates to active executor work where supported | node.ts stream abort listener; mcp.ts signal forwarding | Live cancellation recorded, but Node stream-creation window is uncovered | FAIL |
| BFF uses request-scoped selected browser session; no browser credential | ui/load POST; session.client(); shared Rust browser-session Bifrost branch | Recorded live BFF denied/cross-tenant proof and Rust focused test | PASS |
| Four-host foundation, built AppBridge artifact, gallery, runnable custom app | app/main.ts; view.ts; examples/ui-app; _gallery | Recorded scenario 4 and human acceptance 2026-10-20 | PASS subject to defects above |
| Degraded terminal parity and production Linux packaging | Shared collector preserves terminal; bundle packs SDK/native addon | Explicitly not produced live for degraded terminal; Linux builds/images not run locally | Unverified limits, not claimed green |
| Non-goals stay excluded | No saved view store, Ask service, A2UI adapter, production MCP registration or mutation dispatcher | Cumulative source/history inspection | PASS |

## Proposed findings

### INV-003-1 — INCORRECT: inherited view range overrides an explicit chart range

- Obligation: REQ-030, view document: “Panels inherit it unless their input sets one.” SQL execution must preserve truthful bounds.
- Location: `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/loaders.ts:160` (`loadChart`).
- Producer → consumer: ChartInputSchema accepts an explicit `input.range`; WyrdView always supplies the resolved document range (and BFF forwards that context); loadChart chooses `context.range ?? input.range`, so the explicit range never reaches chartParams or the query. Direct loaders without context happen to work, creating host-dependent behavior.
- Reproduction: read-only Node execution against existing built package with document bounds Oct 1–20 and panel bounds Oct 10–11 emitted Oct 1–20 for SQL `SELECT $1, $2`. Source independently establishes the same expression.
- Consequence: an authored fixed comparison panel queries and displays the wrong time interval in document/BFF/MCP hosts, and Open in Query faithfully reproduces the wrong interval.
- Smallest correction: select the panel's explicit range before inherited context in the shared loader; preserve once-resolved inheritance for panels lacking an override. No host-specific range guard or backend change. Add a focused override/inheritance test through WyrdView/loader, including an absolute panel range and a relative panel range.

### INV-003-2 — INCORRECT: chart validation and rendering select different duplicate columns

- Obligation: REQ-030 preserves duplicate names positionally and renders incompatible chart schemas as panel errors; REQ-025 numeric/time/text shape rules.
- Location: `.../packages/ui/src/lib/loaders.ts:127-131`, versus `.../packages/ui/src/lib/plot.ts:18-19` and all its callers.
- Producer → consumer: normalized query results legally carry duplicate names; `new Map(columns.map(...))` keeps the **last** column's type, while `columnIndex` uses the **first** matching position for timeseries, bar and stat rendering. A first text `y` and second numeric `y` passes validation but the plot consumes text; reversed ordering wrongly rejects a usable first numeric column. Existing table rendering correctly remains positional.
- Reproduction: `shapeChart({type:'stat',sql:'SELECT 1',y:['y']}, {columns:[{name:'y',data_type:'Utf8'}, {name:'y',data_type:'Int64'}],rows:[['bad',3]],...})` returns populated. Chart.stat selects index 0 and formats NaN. The same defect reaches timeseries and bars.
- Consequence: allowed query results produce NaN/invalid plot geometry or false schema errors rather than the required honest error/value.
- Smallest correction: reuse the existing first-occurrence `columnIndex` selection in shape validation so the validated type is exactly the consumed position, preserving positional table rendering and normalizer behavior. Test both duplicate orderings for a bound plot column; no renaming/deduplicating result columns or second response shape.

### INV-003-3 — INCORRECT: valid negative numeric charts generate invalid SVG geometry

- Obligation: REQ-025 bar/timeseries accept numeric columns; no nonnegative restriction. REQ-030 requires complete charts and honest schema/results.
- Location: `.../packages/ui/src/lib/components/Chart.svelte:44-70` (timeseries bounds) and `:118,145-149` (bar geometry).
- Producer → consumer: shapeChart admits signed Int/Float/Decimal values; timeSeries/barRows preserve negatives; Chart computes an upper nice bound by taking log10 of a negative or zero high, and bar height directly from signed value divided by positive barMax.
- Reachable examples: two timestamp rows y=-10/-5 make `niceCeil(-5)` NaN; flat y=-10 produces `niceCeil(-9)` NaN. All-negative bars give barMax=1 and negative SVG rect heights. Mixed bars still give negative rect heights for negative categories. Existing positive fixture tests do not cover these cases.
- Consequence: valid SQL result data produces invisible bars or NaN axes/paths instead of displaying signed values. This is numeric correctness, not a visual preference.
- Smallest correction: correct domain and zero-baseline geometry in the existing Chart owner, with finite increasing signed bounds and nonnegative rectangle dimensions extending to the correct side of zero. Preserve labels, nullable behavior, clipping, source and actions. Add focused all-negative/mixed-sign/zero series and bar checks asserting finite coordinates and visible nonnegative-sized bars; do not reject valid signed numbers to avoid rendering them.

### INV-003-4 — INCORRECT: folds five series despite approved twenty-series boundary

- Obligation: REQ-025 explicitly says “more than 20 series fold into ‘other’.”
- Location: `.../packages/ui/src/lib/plot.ts:14-15,48-54`; `.../src/lib/ui-library.test.ts:104-109` asserts the conflicting behavior.
- Producer → consumer: valid query results with five distinct series feed timeSeries, which retains only three and sums two into “other” because MAX_SERIES=4, justified by available line styles. Chart faithfully displays the information loss.
- Reproduction: read-only runtime invocation with five labels returned `['s4','s3','s2','other']`.
- Consequence: five through twenty series lose their identities prematurely, contrary to the platform chart contract; neither canvas anatomy nor green test can redefine the approved threshold.
- Smallest correction: align the existing timeSeries folding boundary with the approved twenty-series behavior, and keep renderer style selection valid for all retained series without introducing another limit. Replace the five-series folding assertion with 5/20 preserved and >20 folded cases; retain deterministic totals/order behavior.

### INV-003-5 — MISSING: CardHeader version/tab actions discard host-reported outcomes

- Obligation: REQ-030 component actions: host-supplied pending/success/denied/error, duplicate prevention while pending, visible refusal/stable errors, unavailable reason; TASK-003 requires declared CardHeader action results.
- Location: `.../packages/ui/src/lib/components/CardHeader.svelte:54-68,103-112` and WyrdView::act.
- Producer → consumer: WyrdView validates version/tab intents and stores returned ActionState in `panel.actions`; CardHeader receives that map but reads it only for `card.open_in_observe`. Version select and tab buttons ignore pending/error/denied/unavailable states. Direct component consumers also cannot enforce state through supplied actions alone.
- Reachable path: host returns `{state:'denied',problem:{code:'WYRD_PERMISSION_403_DENIED_RBAC',title:'Permission denied'}}` to a version or tab intent; WyrdView stores it, but no rendered element reads it. A browser native select can visibly change to another version while the header still shows the old Card and no explanation. WyrdView suppresses duplicate callback dispatch in its own host, but the component control stays enabled and direct hosts have no built-in duplicate prevention.
- Consequence: the caller sees an apparent silent navigation failure and can repeatedly trigger actions in direct component hosts; the stable refusal is lost.
- Smallest correction: consume the two existing ActionState entries at the CardHeader controls and render their feedback with the existing action-state convention; preserve the native select, exact identity and current tabs. No new dispatcher or server surface. Test version and tab controls with pending plus denied/error/unavailable outcomes, and host-driven WyrdView delivery.

### INV-003-6 — INCORRECT: cancellation during Node query creation misses the abort handler

- Obligation: REQ-030 supersession propagates cancellation to active executor work where supported.
- Location: `.../packages/ui/src/lib/node.ts:29-36`.
- Producer → consumer: run checks signal before read; read then awaits Bifrost connection and stream creation; the signal may abort during either await. The abort listener is installed only after stream resolves. AbortSignal does not replay an already-fired abort event, so stream.collect proceeds without ever calling return. Existing stream.return supports cancellation and is already the chosen owner for it.
- Consequence: superseded or departed panels can keep a query collecting and consuming server/native resources until natural completion/deadline; stale display guards prevent overwrite but cannot cancel that work.
- Smallest correction: in the existing executor, close a newly acquired stream when its signal has already aborted and reject cancellation before collection, while retaining listener/finally cleanup for later aborts. Do not add retries or another cancellation layer. Focused proof can defer stream creation, abort before resolution, then assert stream.return occurs and collect is not started; also retain ordinary post-creation cancellation proof.

## Verification and limits

Ran only a read-only `mise exec -- node --input-type=module` reproduction using the existing built loaders/plot modules: range precedence, duplicate-type validation and five-series folding all reproduced. Source is the authority; the built output agreed with it. No new test files, source modifications, broad test reruns or live infrastructure were created for discovery. Recorded scenario and build evidence was inspected rather than relabeled as independently executed. Human visual acceptance is honored and does not waive the data/action invariants above. Degraded live parity and Linux bundle/image closure remain the explicitly disclosed proof limits; no speculative infrastructure finding is proposed for them.

No prior remediation ledger was supplied. All IDs above are discovery-local proposals for independent validation, not final FIND IDs. No optional stylistic notes are proposed.
