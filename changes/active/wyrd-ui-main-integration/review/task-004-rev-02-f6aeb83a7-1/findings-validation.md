# Independent findings validation

**Recommendation: FIX_REQUIRED.** Ten material findings remain after source validation and consolidation. No specification change is needed for the selected corrections. This is an acceptance audit of the existing assignment, not authorization for a new visual design or data architecture.

## Subject, independence and coverage

Repository `/home/thorrester/Documents/GitHub/wyrd`; base `4a76331113864f9bddc43d3cc8df01080a696937`; candidate `f6aeb83a77179ac4450110671df6f564adc1914a`. First review of TASK-004-REV-02 under approved specification revision 18. No prior task-review acceptance carries forward. Retired R1 remains inherited scope/evidence, not acceptance of this candidate. HEAD and tracked source stayed unchanged during validation.

Read all five selected reports: `task-review-behavior.md`, `maintainer-review.md`, `standards-review.md`, `domain-review-security.md`, `system-review.md`; also `subject.md` and `verification.md`. Independently inspected the original task, governing specification sections, inherited shell task, UI/SvelteKit guidance, mandatory repository rules and the security permission-check authority. CodeGraph preceded code navigation. Inspected actual source/caller paths rather than accepting reviewer agreement: conventional route/server/layout/host, existing example server-load host, WyrdView startup/refresh/actions, shared loader registry, metric planner/list projection, plot shaper and Chart consumer, RecordTable states/actions, canonical metric schema, JSON-result owner, canonical patch and existing launcher, mise process owner, predicate service/HTTP/parser and Python native/stub/facade seam. Main and Metrics PNGs and TimePicker HTML independently corroborate the missing composition; the behavior report records all six reference boards. No rendered-candidate screenshot comparison is claimed here.

No new blocker was discovered by the validator; every retained issue originates in a discovery report. No focused follow-up is needed: duplicate SSR proposals describe the same source path; differences between the implementation narrative and actual plot behavior are resolved directly from source. All selected reports are present. Rejected diagnoses and narrowed recommendations below create no additional work.

## Proposal disposition

| Discovery source | Disposition | Stable finding |
|---|---|---|
| B-01 | CONFIRMED; preserve shared data components and build only missing app shell composition | FIND-TASK-004-REV-02-1 |
| B-02, B-05, MAINT-01 | REVISED/consolidated: one conventional host correction owns server props, applied URL range and coordinated refresh | FIND-TASK-004-REV-02-2 |
| B-03 | CONFIRMED; generic shaper sums duplicate instants and folds overflow, rather than merely dropping points | FIND-TASK-004-REV-02-3 |
| B-04 | CONFIRMED; exact nested projection must be demonstrated against real SQL, not assumed | FIND-TASK-004-REV-02-4 |
| SYS-01 | CONFIRMED; scope is post-readiness setup failure, not a new supervisor | FIND-TASK-004-REV-02-5 |
| SYS-02 | REVISED: restore actual trace identity at the canonical application/driver seam; no Wyrd protocol/header/SDK change | FIND-TASK-004-REV-02-6 |
| RS-01 | CONFIRMED; only the new native declaration is missing | FIND-TASK-004-REV-02-7 |
| RS-02 | CONFIRMED; two changed fallible helpers, no unrelated doc cleanup | FIND-TASK-004-REV-02-8 |
| RS-03 | CONFIRMED; scrub state/caller/body when adding required handler span | FIND-TASK-004-REV-02-9 |
| SECURITY-01 | CONFIRMED as an explicit mandatory authority violation; no current privilege escalation established | FIND-TASK-004-REV-02-10 |

## Validated ledger

Paths described as UI-relative begin at `crates/wyrd/wyrd-server/wyrd-ui/`.

### FIND-TASK-004-REV-02-1 — Approved canvas shell and Observe composition are missing

**Sources/status/classification:** B-01; CONFIRMED; MISSING. **Obligation:** specification REQ-007/015/016/017/020/021/039, original task “Real shell and all three Observe tabs,” inherited TASK-012/020/021/022 and R1 composition/interaction requirements.

**Location and producer-to-consumer evidence:** tenant `src/routes/t/[tenantKey]/+layout.svelte:9–39` renders a horizontal header, not a persistent navigation rail/breadcrumb. Observe `+page.svelte:44–107` implements separate native range/custom/group/live controls and WHERE form. `src/lib/observe-host.ts:13,50,146–176` provides five presets, StatStrip overviews, facets/list and a selected metric's disconnected Chart and generic SQL RecordTable. `RecordTable.svelte:125,164–180` neither opens metric detail nor offers Columns for traces; there is no Metrics series selection shared with Chart, no metric search/picker rail, no trace Filters/CSV composition, and no three trace overview Charts or Logs severity timeline. Existing detail rendering does reuse TraceWaterfall/RecordDetail/Drawer. Main, Metrics and TimePicker references substantiate these omissions; they are not subjective spacing preferences.

**Consequence/materiality:** every signed-in user receives a different investigative workflow from the agreed boards. Data flowing into a table cannot substitute for missing overview/selector/series interactions. Required component and per-page visual evidence is absent; the task itself records human acceptance pending and the caller has rejected it.

**Decision-complete correction:** assemble the named Main, Spans, Logs, Metrics, TraceDrawer and TimePicker compositions using the existing package `Chart`, `FacetRail`, `RecordTable`, `TraceWaterfall`, `RecordDetail`, `Drawer` and presentational primitives. Missing shell controls belong in the app, outside the package/catalog: one reusable app TimePicker used by all Observe signals, left rail, breadcrumb/top bar and consolidated WHERE/zoom/refresh/Live/group controls. TimePicker must supply the board's presets, custom range, UTC/local selection, validation and recent committed ranges; it commits through the single host state in finding 2. Reuse existing theme/session/tenant/settings/logout owners. Undelivered destinations remain explicitly disabled; no Ask entry or invented destination workflow. Use current brand tokens/local fonts, not literal board colours/font-service imports/shadows that conflict with brand authority.

Provide the three shared-loader-driven trace overview Charts and severity-stacked Logs timeline in the board regions. Compose the existing tables/facets and preserve existing hold/new-row/detail semantics. Metrics uses the board selector/filter rail, compatible recorded-dimension grouping, chart and selectable exact series summary. Extend the existing shared Chart/RecordTable input/action mechanism only for missing required series selection/visibility: a stable series key selects the same row and line, and both use the exact same shaped series dataset from finding 3. Do not hand-render a replacement table/chart in the page or put the shell into the reusable data catalog. Preserve StatStrip's other approved consumers rather than changing its published inputs to imitate an overview chart. Aggregate controls expose only genuinely supported operations; unsupported percentile/rate/plot choices remain visibly unavailable. A canvas illustration cannot override the explicit prohibition on invented percentiles.

**Burden and rejected smaller alternatives:** app-only shell assembly and bounded shared component extensions are required because the controls/composition are absent. A cosmetic header change, raw point table or renamed StatStrip does not supply the missing workflow. No new UI kit, transport, renderer or analytics service is justified.

**Focused closure:** compare the six named boards at their reference desktop viewport and the same representative data state, with implementation/reference captures and recorded differences. Exercise the newly supplied controls, series visibility from summary and legend, shared overview zoom, Filters/Columns/CSV, drawer and TimePicker. Cover affected populated/loading/empty/denied/error/partial, light/dark, mobile, keyboard/focus, touch and long-content/overflow states. Preserve separate component evidence and human acceptance; automated data reads and screenshots alone are not human approval. Use the existing deployed BFF/browser test owner, not a new runner or full default batch replay.

### FIND-TASK-004-REV-02-2 — Conventional host does not own server-loaded, shareable, coordinated investigation state

**Sources/status/classification:** B-02, B-05, MAINT-01; REVISED/consolidated; VIOLATION and INCORRECT. **Obligation:** REQ-030 use case 1 at `spec.md:900–904`, REQ-039 and original task single-range/predicate, restorable range/timezone, cancellation and live investigation requirements.

**Location and trace:** Observe `+page.server.ts:13–22` returns only predicate metadata. `+page.svelte:6,13–17,109–111` passes documents to WyrdView/remoteLoader. `WyrdView.svelte:62,252–270` initializes no panels and starts them only in effects, so initial authenticated HTML has no signal evidence. `observe-host.ts:153–175` marks only list input live; renderer `refresh:279–285` re-resolves that list's relative range while charts/facets/summary retain the original range. The existing renderer intercepts `chart.range` into local `brushed` state at `:308–310`; host `step:203–229` never serializes it. Reload/signal navigation loses the brush. These are reachable default and live Observe paths, not dormant mechanisms.

**Consequence/materiality:** required conventional content waits for browser hydration and per-panel POSTs; appended rows can describe a newer window than the associated charts/facet counts; zoom cannot be shared/restored. This violates explicit server/client and investigation-state boundaries, not a general demand to optimize every page.

**Decision-complete correction:** run the existing shared typed loaders (or existing `loadPanel` validation/dispatch where appropriate) from authenticated Observe `+page.server.ts` with the existing session-bound `nodeExecutor`. Return typed component props and directly compose package components, as the existing `examples/ui-app/src/routes/+page.server.ts` and sibling page already demonstrate. Keep SQL, joins, dependent-read shaping and metric semantics in shared owners. Remove only conventional document-only planning no longer needed; preserve WyrdView/remoteLoader and `/ui/load` for gallery/view-document/custom/MCP consumers. Do not add a preloaded-results/SSR mode or result-bearing view-document format to repair the wrong host.

The conventional host owns one applied URL state for range/timezone/predicate/facets/group/mode/exact selection/return. TimePicker, brush, zoom/reset and navigation commit into it. Resolve relative bounds once per refresh generation and pass those same absolute bounds and effective predicate to all dependent chart/facet/list/summary loaders. Use SvelteKit navigation and scoped invalidation with existing abort propagation; do not make per-widget timers/controllers the investigation authority. Keep prior successful data visible on an invalid unapplied draft with a visible refusal and no dependent reads. Preserve RecordTable's existing held rows/counts, new-row notice and resume mechanism; refresh must update incoming props without discarding held state. Aborted or superseded generations cannot overwrite newer props.

**Burden:** one app host/server-load correction closes all three proposals; no second query/data lifecycle is needed. Merely adding server predicate validation or making every WyrdView panel live fails server-rendered composition, URL restoration and single-generation ownership.

**Focused closure:** inspect authenticated initial HTML (or a JS-disabled browser) for actual populated/empty/denied/error component output, including direct exact-record URLs. With JS enabled, verify no duplicate initial panel-loading owner; range/timezone/brush/reset survive reload, signal switch and Back. Append an observation under controlled time and assert all panel reads in a refresh use the same absolute bounds/predicate, late results are ignored, and existing held rows/counts/new-row/resume remain intact. Extend the existing focused V3/V4–V6 tests rather than add a new framework.

### FIND-TASK-004-REV-02-3 — Metric series identities and numeric summaries corrupt the represented evidence

**Source/status/classification:** B-03; CONFIRMED; INCORRECT. **Obligation:** REQ-017/039 and task precision paragraph expressly disallow generic duplicate summation/series folding as metric aggregation and require truthful compatible series/Latest/Max/Mean.

**Location and trace:** `packages/ui/src/lib/observe.ts:311–315` produces a caption using optional stringified group plus type/unit/temporality/monotonicity, omitting recorded point/resource/scope dimensions present in `crates/vala/vala-bifrost-redux/src/tables/metrics/points.rs:111–153`. `metricPanels:333–360` uses that caption for raw Chart and summary SQL. Chart calls `plot.ts::timeSeries` (`Chart.svelte:44–45`); `plot.ts:38–47` **sums** equal-time points, and `:52–63` sums excess series into other. This is not the recorded narrative's single-point retention. `metricPanels:358–359` COALESCEs integer and double aggregates: samples int 1 and double 100 in the same legitimate scalar series yield max/mean 1 rather than 100/50.5. Stringifying a group and substituting `not set` additionally merges null and that literal string.

**Consequence/materiality:** ordinary gauge instances with different attributes become one line and implicit sum, while summary mean remains over raw samples; >20 series silently gain an aggregate. Mixed scalar representation omits valid samples from max/mean. Exactness labels do not make these values correct.

**Decision-complete correction:** extend the existing shared Metrics planning/shaping owner. Preserve typed series identity separately from caption, including compatible metric descriptor and recorded resource/scope/point dimensions, subject to existing authorization. Use that same identity/data selection for plot, exact summary, legend and selection/visibility from finding 1. For the unaggregated metric mode, preserve recorded series and points with **no implicit duplicate-time sum and no other-series fold**. Keep generic authored-query Chart aggregation behaviour for its separately approved consumers; add only the narrow shared metric-specific shaping path needed to avoid those semantics. Bounded series presentation may show a truthful selected subset/count with access to other retained series; it cannot invent their summed line. Any explicit supported aggregation must state and use the same operation for chart/summary; unsupported choices remain unavailable. Compute Latest/Max/Mean over both scalar representations through existing SQL/result capability, retaining decimal-text evidence and marking double plot geometry approximate. Do not define identity by numeric representation or a human label, create a new wire encoding, or bypass payload permissions to obtain dimensions.

**Burden:** changing only Chart's caption or summary footer cannot recover distinct series already merged upstream. Bounded shared metric planning/shaping and component extension is the smallest complete correction; a generic metric framework is unnecessary.

**Focused closure:** real query projections and focused shape/component tests cover same-instant distinct point/resource dimensions, null versus literal `not set`, mixed int/double, unsafe integers, missing versus zero and >20 series. Assert exact expected max/mean, no implicit folding/sum, and identity/line/summary selection agreement. Preserve existing generic authored-query plot tests. If exact plot geometry is impossible, disclose approximation or refuse that plot rather than replace exact display.

### FIND-TASK-004-REV-02-4 — Recorded metric structures are inaccessible or lose integer precision

**Source/status/classification:** B-04; CONFIRMED; MISSING/INCORRECT. **Obligation:** original task explicitly retains histogram, exponential histogram, summary and exemplar shapes and requires lossless nested-count projections through existing SQL/results; REQ-017/039.

**Location and trace:** `observe-host.ts:50,151–165` hardcodes only six extra structure columns; `schemas.ts:149–153` already inventories additional exponential fields, summary_sum and exemplars. Metrics has neither record-open nor Columns controls (`RecordTable.svelte:125,164`), so these additional fields have no shipped inspection path. `observe.ts::listQuery:289–292` casts only scalar `EXACT` columns (`:302`), leaving bucket_counts numeric. Canonical metric schema declares bucket elements and nested positive/negative bucket counts as Int64. The existing JSON-result path and JavaScript JSON numbers cannot preserve every such integer. Small `[1,2]` fixtures do not prove `9007199254740993` survives.

**Consequence/materiality:** valid retained structures disappear from the investigation and large nested counts can silently change. This is an explicit evidence requirement, not optional support for an unrelated metric instrument.

**Decision-complete correction:** use the existing shared metric schema/column inventory and loader/projection owner to make all required recorded structures inspectable in the existing RecordTable presentation/composition. Do not add another metric detail service or duplicate hardcoded partial column set. Preserve exact scalar casts and project unsafe nested integral values as decimal text through existing SQL/results before the JSON boundary, retaining shape and kind metadata and null/zero distinction. The implementation must prove that projection against actual SQL; this validation does **not** claim a specific nested transform is already proven. Existing task stop condition applies if required lossless projection demonstrably needs a new wire encoding/public boundary: report the concrete unsupported operation and route that decision, rather than silently rounding or authorizing Arrow/new transport. Unsupported plotting remains explicitly unavailable while exact recorded structures remain inspectable.

**Burden/proof:** reuse the existing schema inventory, query/projection owner and table; plain omission or approximate chart labels are insufficient. Seed explicit/exponential histogram, summary and exemplar shapes containing unsafe nested integer/count values and absent/zero cases; assert exact visible retained values through the real projection. No full environment replay or new parser/transport is required to demonstrate this gap.

### FIND-TASK-004-REV-02-5 — Setup failure destroys healthy diagnostic services

**Source/status/classification:** SYS-01; CONFIRMED; MISSING. **Obligation:** approved REQ-038 explicitly keeps healthy services available after setup/batch failure; original task Scenario 1.

**Location/trace:** `mise.toml:1288,1296–1303,1323` runs under `set -Eeuo pipefail`, installs EXIT cleanup, starts mock/server/UI, then invokes setup unguarded. Existing `scripts/dev/example-app.sh::setup` runs deployment/declaration under `set -e`; failure propagates into EXIT cleanup and terminates the healthy services/ephemeral database. The later batch command is already guarded and retains services. Normal setup refusal is reachable.

**Consequence/materiality:** the same transient deployment/Card error destroys the environment needed to inspect it, contrary to the explicit recovery promise.

**Decision-complete correction:** use the existing process/status owner and batch-failure pattern for post-readiness setup failure: report setup failure, leave already healthy services available, skip dependent app/batch launch and do not claim their readiness. Preserve existing app-first graceful shutdown and owned-child cleanup on interruption; earlier unusable-startup failures retain their existing fail-fast behaviour. No new supervisor, retry/recovery controller or persistence contract.

**Burden/proof:** bounded existing launcher branching; changing EXIT cleanup globally would weaken valid cleanup. Add one focused post-readiness setup-failure case asserting healthy server/UI/mock remain inspectable, app/batch are not launched, and Ctrl-C cleans owned processes. Full default batch replay is not closure proof for this branch.

### FIND-TASK-004-REV-02-6 — Batch success uses ambient counts instead of actual workload membership

**Source/status/classification:** SYS-02; REVISED; INCORRECT. **Obligation:** original task finite-batch section requires actual request/standard trace context, correlation by existing trace/Run IDs, and failure for missing telemetry; REQ-038.

**Location/trace:** reviewed patch `scripts/dev/wyrd-example-py.patch:88–98,112–133,155–198` defines Call/send/SPANS/LOGS/JUDGE/readback/missing. Effective canonical `scripts/batch.py` no longer records trace IDs. Plain requests correctly leave roots app-owned, but SPANS/LOGS count all support-desk traces since start, then `missing` compares aggregate counts with workload denominators. JUDGE similarly counts all matching model calls in the time window. Unrelated live requests can replace absent workload telemetry; returned Run/execution IDs only constrain some other counters.

**Consequence/materiality:** supported concurrent investigation/appended workload can produce a successful batch report with missing workload HTTP spans/logs, including no-Run HTTP-only/404 cases. Source proves false membership; the system report's isolated missing-function probe corroborates it. A quiet happy-path batch does not falsify this defect.

**Decision-complete correction:** retain plain app-owned request roots. Restore actual per-response trace identity at the **canonical sample application and its own driver**, using its existing OpenTelemetry ASGI instrumentation response hook to expose the active trace context as response metadata and have send retain that context for all four request cases. This is a counterpart application instrumentation change already inside the approved canonical patch scope, not a Wyrd SDK header, credential, grouping metadata, new wire encoding or query API. Do not restore invented upstream parents or create another Run/exporter. Reuse public Bifrost readback, constrain required HTTP/log membership to those captured actual traces, and model/judge/verdict membership to returned Run/execution identities. Report missing individual membership; ambient counts may remain diagnostic totals but cannot establish completeness. Keep metric points as separately reported bounded-window observations, not one-per-request evidence.

**Burden and narrowing:** counts/threshold guards cannot reconstruct identities the producer discarded. A small canonical ASGI-to-response-to-driver bridge is the minimum source correction for HTTP-only/error calls; Run IDs alone cannot identify them. No Wyrd protocol change or new architecture decision is selected. Do not expose or log credentials in that context metadata.

**Focused closure:** a small canonical app/driver test captures real context for shipping/refund/openapi/404, verifies app HTTP roots remain roots and their required public retained evidence is matched. Include unrelated traffic during readback, omit one workload identity from the checked retained data, and assert the report names the missing workload evidence instead of accepting unrelated rows. Keep original request denominator, expected verdicts, production wheel/source provenance and report categories. No full 3,000-request replay is needed for this targeted correction.

### FIND-TASK-004-REV-02-7 — New native Python method lacks authored declaration

**Source/status/classification:** RS-01; CONFIRMED; VIOLATION. **Obligation:** Python AGENTS six-layer/native authored stub parity.

**Location/trace:** `sdks/wyrd-sdk-python/src/bifrost/mod.rs:979` exposes native validate_predicate; public synchronous/asynchronous facades call it at `_client.py:1110,1436`. Existing native `python/wyrd/_wyrd/bifrost.pyi:64–107` has no method declaration and falls through `__getattr__ -> Any`. Its public facade classes are already inline typed and re-exported correctly.

**Consequence/materiality:** a new Python boundary bypasses the explicit authored signature gate; current typecheck passes because Any masks the omission. This does not authorize cleaning up all older fallback methods.

**Decision-complete correction/burden:** add only the exact namespace/name/expression-to-str method declaration with required parameter/return documentation to the existing native Bifrost stub. Keep public facade definitions and exports single-owned; do not add aliases or duplicate public class stubs. This is a bounded authored declaration, no runtime machinery.

**Focused closure:** compare native/stub signature, run existing py:lints and py:typecheck; retain existing public predicate runtime journey evidence. No new test harness.

### FIND-TASK-004-REV-02-8 — Changed fallible parser helpers omit required error rustdoc

**Source/status/classification:** RS-02; CONFIRMED; VIOLATION. **Obligation:** root AGENTS changed Rust rustdoc/# Errors gate, including private helpers.

**Location/trace:** new `crates/wyrd/wyrd-server/src/query/floor.rs:307` check_predicate and `:408` check_column each return Result and have only a summary line. Their complete bodies reject unsupported expression/column states through QueryInvalidSql. normalize_predicate calls them on the public validation path.

**Consequence/materiality:** explicit mandatory merge gate remains unmet; no probability argument waives it.

**Decision-complete correction/burden:** complete these two existing doc blocks with validation role, invariants/rejection conditions and # Errors describing their stable invalid-SQL outcomes. Preserve behaviour; no refactor or new tests for documentation. Batch with this behavioural remediation rather than a style-only round.

**Focused closure:** source inspection and owning format check; changed Rust behavioural checks cover compilation already.

### FIND-TASK-004-REV-02-9 — New public handler lacks required scrubbed tracing

**Source/status/classification:** RS-03; CONFIRMED; VIOLATION. **Obligation:** root AGENTS server handler tracing::instrument with scrubbed arguments.

**Location/trace:** mounted POST `crates/wyrd/wyrd-server/src/query/routes.rs:191` validate_predicate has no instrument attribute. It accepts state, caller and predicate-bearing body, then delegates the existing service. Nearby Bifrost handlers demonstrate existing scrubbed instrumentation.

**Consequence/materiality:** an explicit public-boundary merge gate is absent. Default Debug instrumentation would also risk exposing predicate/caller material; the correction must scrub it.

**Decision-complete correction/burden:** instrument the existing handler with the established tracing mechanism, skipping state, caller and body. Preserve existing errors/service dispatch and log no raw predicate or credential data. No tracing wrapper, telemetry schema or dependency is necessary.

**Focused closure:** inspect span/skip list and run affected existing Rust compile/lint/predicate checks. No new harness or broad observability suite.

### FIND-TASK-004-REV-02-10 — Predicate handler duplicates permission checks outside the configured authority

**Source/status/classification:** SECURITY-01; CONFIRMED; VIOLATION. **Obligation:** `architecture/wyrd-security-posture.md:576–592` makes PermissionCheck the runtime handler chokepoint and prohibits shortcut helpers.

**Location/trace:** `crates/wyrd/wyrd-server/src/query/service.rs:346–347` closes over principal.effective_permissions.contains, then uses it for exact table permission and gateway payload permission before describing/normalizing. HTTP, MCP and all SDK validation requests reach it. Existing QueryAuthority::authorize uses state.auth.permission_check.check; runtime PermissionVerdict and RbacCheck already own the decision.

**Consequence/materiality:** explicit sensitive-boundary authority rule is violated through a second handler implementation. Current RbacCheck is equivalent contains logic: no different current allow/deny result, privilege escalation or tenant leak is claimed. The narrow mandatory correction is still required.

**Decision-complete correction:** use the existing configured checker for the already-resolved exact table-scoped Permission and existing gateway payload Permission. Preserve coarse admission, authenticated tenant/table resolution, permission-before-description, scoped grant success, current stable table/payload refusal and nonblocking audit owner. Do **not** call QueryAuthority::authorize unchanged because its global permission would reject scoped readers. No new checker, policy model, cache or permission type.

**Burden/proof:** replace two existing boundary decisions with the configured mechanism; deleting checks weakens authorization, widening to global permission changes required access. Static call tracing plus existing focused Rust/MCP predicate tests prove scoped success, other-table refusal before column disclosure and payload distinction; ordinary query authorization remains unchanged.

## Reconciled acceptance and verification limits

| Obligation group | Reconciled result |
|---|---|
| Approved shell/boards and individually inspected components | FAIL — finding 1; no human acceptance inferred from automated evidence |
| Conventional SSR, shareable state, unified range/live/zoom | FAIL — finding 2 |
| Truthful compatible metric series and exact structured evidence | FAIL — findings 3–4 |
| Canonical source/current production wheel, ordinary request/verdict/export flow | Source-backed implemented portions remain; complete acceptance FAIL — findings 5–6 |
| Predicate grammar/bounds, tenant-qualified catalog lookup, permission-first schema disclosure and first-class transport projections | Source-backed implemented; mandatory configured-check/stub/handler/docs closure FAIL — findings 7–10 |
| Exact trace/root/detail semantics, retained log controls/detail/correlation, generic names/typed groups, exact SQL/binds Query inspector | Existing source-backed successes in behavior/maintainer/domain reports retained; no replacement required. Recheck affected consumers after shared/host changes. |
| No new durable schema, alternate query authority, credential owner, provider, production fixture fallback or unsupported platform mechanism | No contrary material evidence in selected coverage; preserve as constraints |

Fresh recorded checks: root V3 and V7 each passed their one selected test; domain ran the two exact parser tests successfully; repo py:lints, py:typecheck and diff check passed. Their assertion scope is documented in the respective reports. This validator performed static source/caller validation and reference inspection; it did not rerun production-BFF browsers, Postgres/Docker, default development replay or all language journeys. Green checks do not close the source-proven omissions, and prior V1–V13 claims do not prove the missing board/SSR/precision/failure cases.

No implementation/source edits, merge, commit, push or deployment occurred. Only this report was written. The next review should inspect these finding closures and the remediation diff with affected consumers; it should not restart unrelated family discovery. Full change-level journey aggregates and final default replay belong to integrated change review, not this focused remediation task.
