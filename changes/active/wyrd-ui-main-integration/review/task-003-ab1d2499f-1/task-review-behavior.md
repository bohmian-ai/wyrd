# TASK-003 independent behavior review

Result: **FAIL**.

## Immutable subject and authority

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; base `695ca914df61a8683059f6f32279ec2148424e06`; candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`. Reviewed the cumulative range and resulting source, including the client and bundle changes, rather than only the latest visual revisions. TASK-027 is an approved successor and is not classified as foundation drift. The four user-excluded commits and upstream merge changes were distinguished using history and path diffs; no finding below concerns them.

Authority: approved spec revision 12, original TASK-003, AGENTS.md, agent-rules, spec-driven-development, maintainer-style, testing-workflows, UI skill, doctrine, and applicable client, query-terminal, browser-session and local-bundle sections of the design/Bifrost/security/deployment authorities. Human visual approval on 2026-10-20 is accepted as supplied; it does not change the specified chart semantics or action contract. No peer report was read.

## Navigation, consumers, and reuse assessment

- `schemas.ts` owns catalog inputs/actions and document validation; `scripts/catalog.ts` derives the advertised catalog. BFF, remote loader, MCP App, gallery and custom app all reach these declarations.
- `loaders.ts` plans/shapes reads. `loadChart` is consumed by `loadPanel`, the custom app server and journeys; `loadCardHeader` has the same consumers. WyrdView and remote hosts use those loaders; direct components receive their resulting props.
- `node.ts` adapts the existing SDK Cards/Bifrost/Execution handles; `mcp.ts` adapts the registered MCP tools. The actual registry tool is `cards.get`, and the adapter correctly extracts its Card envelope. Existing server authorization and normalized query result owners are retained.
- The existing SDK `BifrostQueryStream.collect` and `return` own bounded normalization and cancellation. Node consumes these instead of adding a result/stream implementation. The cancellation finding below is in that adapter, not a request to duplicate this owner.
- Chart's `shapeChart` and `plot.ts` consume the same positional result but currently resolve duplicate names differently. `columnIndex` is the existing first-column mechanism used by timeseries, bars and stats.
- `CardHeader.svelte` has the same `ActionProps` contract as Chart. `ActionButton` already implements disabled pending/unavailable states and visible errors. WyrdView validates and records action outcomes; direct hosts supply those outcomes through `actions`.
- `BrowserSession.client()` forwards the selected Rust-owned session through `WyrdClient.forBrowserSession`; the private query-only Bifrost constructor reuses the existing client/writer shape. No second session, transport, query service or registry owner was found in these paths.
- Self-contained artifact generation extends the existing Vite/package build. The example has direct-component and document pages; credentials remain in its server executor. Production MCP registration and application navigation destinations are later-task concerns; they are not requested as remediation here.

## Acceptance matrix

`PASS` records source support plus the stated proof; it does not claim that historical live checks were rerun in this review.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Approved package entry points and browser/native separation | `package.json`, `index.ts`, `contract.ts` type-only imports, `node.ts` isolated runtime SDK import | Package built during rerun; orchestrator Svelte check and historical app builds | PASS |
| Generated catalog; schemas alone authorize inputs/actions | `schemas.ts`, `scripts/catalog.ts`, `validatePanel`, `validateIntent` | Rerun scenario 1; historical `codegen:check` | PASS |
| Version/catalog pins and malformed document rejection | `ViewDocumentSchema`, `validateView` | Rerun scenario 1 checks both pins and invalid range | PASS |
| Invalid component/input fails only its panel | `WyrdView.start`, `validatePanel`, `settle` | Rerun scenario 1 mixed valid/invalid mount | PASS |
| All Chart types, honest loading/empty/denied/error and exact Open in Query SQL/binds | `loadChart`, Chart, `chartParams`, ActionButton | Rerun scenarios 1/3; historical Node/BFF live reads | PASS except separate chart defects below |
| Chart numeric bar values render correctly, without an unstated nonnegative restriction | Chart `barMax` and `barCells` | Existing test covers positive values only; source gives negative SVG height for negative input | FAIL: B-003 |
| REQ-025 series threshold: more than 20 fold into other | `plot.ts::MAX_SERIES/timeSeries`, Chart styles, scenario-1 assertion | Executed in-memory check: five labels become `e,d,c,other` | FAIL: B-001 |
| Duplicate names stay positional and chart schema errors are honest | `shapeChart` versus `columnIndex` and Chart consumers | Executed check: first `v:Utf8`, second `v:Int64` passes shape and plots NaN | FAIL: B-002 |
| Exact Card lookup then resolved identity listing, pagination and separate listing failure | `loadCardHeader`, CardHeader | Rerun scenarios 1/3; historical populated/denied live paths | PASS |
| CardHeader copy identity/UID, tabs and exact version intent | CardHeader `ref`, copy, version/tab callbacks | Scenario 1 copy assertions; source tracing | PASS for intent values/copy |
| Every declared CardHeader action observes pending/denied/error/unavailable completion | CardHeader version selector and tabs ignore their `actions` entries | Scenario 3 exercises only Chart action outcomes; source read confirms missing consumers | FAIL: B-004 |
| Denied prerequisite causes no dependent read | `loadCardHeader` returns before listing | Rerun scenario 3 | PASS |
| Superseded/removed/unmounted views cannot accept stale results | WyrdView controller generations and cleanup | Rerun scenario 3 replacement/late-result proof | PASS |
| Cancellation reaches supported active executor work, including startup race | `node.ts::read` query branch; SDK stream `return` | Executed deferred-stream check returns success and never closes after abort during startup | FAIL: B-005 |
| Resolve relative range once for every view panel; brush applies view-wide and reset is available | `start`, `resolveRange`, `act`, reset control | Rerun scenario 3; source tracing | PASS |
| Six read adapters and positional normalization share server/SDK owners | `node.ts`, `mcp.ts`, SDK collect, pre-existing shared JsonResultBuilder | Historical live scenario 2; source confirms mappings, error projection and ceilings | PASS with proof limits below |
| Credential/tenant boundary stays at BFF/session/SDK owners | BFF `POST`, selected `BrowserSession.client`, component validation | Historical live scenario 4 includes foreign Card, malformed inputs and unauthenticated refusal | PASS |
| Browser session Bifrost connects without ingest dial | `facade.rs::connect_with/read_only/from_sink`, `is_browser_session` | Historical exact Rust regression and live BFF query | PASS |
| Built MCP App and custom app consume portable library | `src/app/main.ts`, Vite artifact, examples routes | Artifact rebuilt here; historical AppBridge and custom-app live checks | PASS for renderer/read foundation |
| Individual visuals accepted light/dark, desktop/mobile | Gallery/shared fixtures and example/AppBridge hosts | Explicit supplied human approval 2026-10-20 | PASS |
| Native production closure on three supported targets/images | `scripts/build.sh`, `ts:build`; Linux workflow owned outside excluded change | Only darwin-arm64 closure supplied; Linux/image execution not supplied | FAIL to prove all targets; unverified, not evidence of a Linux defect |
| Degraded parity and cancellation proof are credible | Shared JsonResultBuilder preserved; Chart partial warning retained | Degraded path has shared-source proof, not live output; cancellation journey tests only pre-aborted input | FAIL for B-005; degraded live parity remains unverified |
| REQ-020 references and non-goals | Canvas/reference paths retained; no saved-view store, A2UI, write dispatcher, Ask service, production MCP registration or npm publication added in task | Cumulative diff/history and task scope | PASS |

## Proposed material findings

### B-001 — INCORRECT: folding starts at five series rather than twenty-one

Obligation: REQ-030 imports Chart's REQ-025 shapes; REQ-025 explicitly says more than 20 series fold into other.

Location: `packages/ui/src/lib/plot.ts:14-15,47-54` (paths in findings are rooted at `crates/wyrd/wyrd-server/wyrd-ui/` unless otherwise stated); Chart `STYLES` and its direct indexing; `src/lib/ui-library.test.ts` scenario 1 deliberately asserts folding five values.

Producer-to-consumer evidence: normalized rows enter `timeSeries`; any fifth distinct series value causes selection of only three lines plus the aggregate `other`. Chart then renders only those returned entries. The same function also folds distinct y columns even without a series binding. A direct built-module check with five values produced `['e','d','c','other']`. Every host reaches this shared Chart path.

Consequence: views with five through twenty independently meaningful series silently merge them, losing labels and individual values the approved chart contract retains. The green test encodes the conflicting behavior.

Smallest correction: keep the existing `timeSeries` owner, apply the approved threshold, and make the existing Chart style selection support every returned line without undefined indexing. Preserve the existing aggregate-at-an-instant behavior after the threshold; do not add a chart framework. Correct the conflicting test and exercise 5, 20 and 21+ series through Chart, including the legend/tooltip. Do not merely raise the constant while leaving four-entry style indexing unsafe.

### B-002 — INCORRECT: shape validation and rendering bind different duplicate-name columns

Obligation: REQ-030 requires positional results preserving duplicate names and visible chart schema errors; incompatible result schemas must not become blank/partial success.

Location: `packages/ui/src/lib/loaders.ts:126-146`, compared with `plot.ts:17-19,33-39,63-68` and Chart stat selection at `153-157`.

Evidence: `new Map(result.columns.map(...))` keeps the last column's type per name. Existing `columnIndex` resolves the first matching column; all plot/stat consumers use it. A normalized result `[t:Timestamp,v:Utf8,v:Int64]` with row `[instant,'text',1]` passes `shapeChart` as populated but `timeSeries` reads the first `v` and produces NaN. Reversing the two `v` column types rejects a renderable first numeric binding. This is reachable with authored SQL/join projections already demonstrated by the duplicate-name live journey.

Consequence: a valid first-column binding is rejected, or an incompatible first-column binding passes validation and renders invalid geometry/values instead of the promised panel error. The defect affects timeseries, bar and explicitly selected stat columns.

Smallest correction: validate using the same existing first-positional-column mechanism the renderer uses. Reuse `columnIndex` or equivalent existing first-match behavior at this shaping owner; preserve all duplicate result columns/rows. Add focused first-numeric/last-text and first-text/last-numeric coverage through `loadChart`/Chart for the affected shared resolution. Do not deduplicate or rename returned columns.

### B-003 — INCORRECT: signed numeric bar values produce invalid negative heights

Obligation: REQ-025 bar accepts a text category and numeric columns; it imposes no nonnegative-only limitation. TASK-003 owns all Chart types, completed once for all hosts.

Location: `packages/ui/src/lib/components/Chart.svelte:118-119,144-149` and bar `<rect>` render.

Evidence: `shapeChart` accepts Int64/Float64 signed values. `barRows` preserves them. `barMax` is clamped to at least zero (then 1), and height is `value / barMax * availableHeight`, so `-5` gives a negative SVG rect height. For mixed values the positive bar renders and the negative bar disappears; all-negative results have no visible bars. The aria label can still announce values, masking the incorrect visible result. Existing bar tests include only nonnegative rows.

Consequence: authorized numeric data such as signed deltas is silently omitted from bar charts.

Smallest correction: keep the existing bar renderer, scale over signed extents including zero, and draw every non-null bar with nonnegative geometry from the shared zero baseline. Preserve null labels, dense-label fitting and positive-only behavior. Focused Chart cases for mixed negative/positive and all-negative rows should assert finite, nonnegative dimensions and bar placement on the appropriate side of zero; existing positive/dense cases remain green. Rejecting signed values would introduce an unapproved input restriction.

### B-004 — MISSING: CardHeader version and tab controls never consume action completion

Obligation: REQ-030 Component actions requires host-supplied pending/success/denied/error, duplicate prevention while pending, and visible refusal/unavailable reasons. Original TASK-003 explicitly requires declared action results.

Location: `packages/ui/src/lib/components/CardHeader.svelte:55-60,103-113` (version and tab controls); contrast the same component's Observe ActionButton at `93-98` and `ActionButton.svelte`.

Evidence: both callbacks emit declared actions. The version control disables only for absent host or failed version listing; tabs disable only for absent host. Neither reads `actions['card.version']` or `actions['card.tab']`, and neither renders that action's denied/error/unavailable/success feedback. WyrdView records these outcomes in `panel.actions`, but only Observe's ActionButton observes its entry. Direct component hosts can pass `pending` and still receive repeated version/tab intents. In WyrdView the action function prevents duplicate host calls but the control still gives no pending/refusal completion feedback. Gallery also directly exposes these controls with an actions map. Scenario 3 only checks Chart's ActionButton.

Consequence: a refused version/tab navigation leaves the user without the required stable error/reason; direct package consumers can emit the action repeatedly even while the host reports pending. Selecting a refused version can also leave the select displaying a version different from the unchanged Card.

Smallest correction: consume each declared action's existing ActionState at its actual CardHeader control, using the current shared action feedback mechanism where appropriate while retaining select/tab semantics. Disable pending/unavailable actions and display their outcome/reason, keeping exact identity and active tab derived from authoritative props. Add focused direct-component tests for both actions with pending then denied/error/unavailable and successful completion; include a WyrdView host-outcome assertion. No navigation routes, mutation service or new action framework are necessary.

### B-005 — INCORRECT: an abort during stream startup is missed and returns usable success

Obligation: REQ-030 supersession/cancellation; TASK-003 scenarios 2/3 require cancellation propagation where supported and honest canceled outcomes.

Location: `packages/ui/src/lib/node.ts:29-36,49-57`; existing SDK `sdks/wyrd-sdk-ts/wyrd/src/index.ts::Bifrost.stream`, `BifrostQueryStream.return/collect`.

Evidence: `run` checks the signal once before calling `read`. Query `read` then awaits Bifrost connection and stream startup before attaching its abort listener. An AbortSignal event that fires during either await is not replayed to later listeners. The code never checks `aborted` after acquisition or after successful collection. The SDK stream does expose `return()` as its cancellation mechanism, so this is a supported cancellation path. An in-memory check temporarily substituted `Bifrost.connect` with a deferred stream, began `nodeExecutor.run`, aborted before releasing that stream, then released it: result was `{ok:true}`, `closed:0`, `collected:1`, `aborted:true`. The substitution was restored and no source was changed. WyrdView prevents the stale result from rendering, but a direct consumer still receives it and the supported active query never gets closed.

Consequence: ordinary range/input changes while a remote query starts leave canceled work collecting and can report success to direct executor consumers. The live test's immediately pre-aborted signal cannot exercise this race.

Smallest correction: at the Node adapter's acquisition/cancellation boundary, close an acquired stream immediately when the signal has already aborted; ensure a cancellation that wins before completion rejects rather than returning usable rows. Continue using the SDK stream's `return`; do not add a second transport or cancel unrelated reads. Prove abort while stream startup is deferred, assert exactly that stream is closed/no result accepted, and retain existing during-collection and pre-aborted behavior. A small deterministic adapter test is sufficient for this race; the existing real-server journey remains the transport proof.

## Verification and limits

Executed `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: (Validated portable rendering|Dependent loads and safe actions)$'`: exit 0, two selected tests passed, five excluded; package and self-contained artifact rebuilt. The orchestrator independently reran each exact scenario, the complete seven-test file and Svelte checks as recorded in `verification.md`. A separate `mise exec -- node --input-type=module` in-memory check reproduced B-001/B-002 from built modules and B-005 with a restored deferred SDK method. No source/test files were edited.

Historical task evidence supplies the real Postgres/SDK/MCP/BFF journeys, query/Rust exact tests, codegen/lint/boundary checks, examples and darwin bundle closure. I inspected what the live tests assert: successful five read operations, missing execution's 404, malformed bind, row/byte ceilings, pre-aborted signal, denied query, BFF tenant/credential checks and actual AppBridge loading. They do not produce a live degraded terminal, exercise the startup abort window, or assert CardHeader version/tab action feedback. These limits cannot be promoted to complete proof by green checks.

Linux bundles/images, live degraded parity, and fresh browser visual inspection were not rerun. No source implementation, merge, push or deployment was performed. All proposed findings are behavioral and bounded; there are no style-only blockers or optional redesign requests in this report.
