# TASK-003 independent structured Ponytail validation

Result: **seven retained bounded implementation findings**. No new product or architecture decision is required. Discovery proposals were validated independently against source, not counted as votes.

## Subject, authority and coverage

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; base `695ca914df61a8683059f6f32279ec2148424e06`; candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`. HEAD was checked and remains the candidate. Only this report was written. No prior FIND ledger was supplied.

Read subject/map, claims-comparison, verification, both implementation reports, standards, maintainer, system and security reports. No follow-up report exists; the supplied comparison identifies no material unresolved contradiction. Expanded the map through all proposed correction owners and their callers. Reviewed cumulative range/history and relevant source rather than latest fixes alone. Excluded commits and approved TASK-027 are contextual changes, not foundation findings. No CodeGraph index is available.

Applied AGENTS.md, agent-rules, spec-driven-development, maintainer-style, errors guidance, applicable design/doctrine, browser-session/security and Bifrost ownership authority, original task, and approved revision-12 REQ-025/030. Human visual approval is accepted as supplied; it does not override behavioral contracts. Read the full functions proposed corrections affect: loadChart, chartParams, shapeError/shapeChart, inputProblem/validatePanel/validateIntent/loadPanel, resolveRange, timeSeries/barRows/columnIndex, all Chart geometry/rendering, CardHeader and ActionButton, nodeExecutor/read/run, mcpExecutor/problem, remoteLoader, WyrdView loader/start/settle/act, both panel endpoints/refuse, and existing server problem projection.

The new portable loader/view orchestration is expressly required. Existing SDK Cards/Bifrost/Execution, MCP tools, JsonResultBuilder, browser-session transport and Rust query-only construction already own durable reads, normalization and authorization; keep them. No finding asks for another client, query engine, renderer framework, action dispatcher, credential owner, retry policy or persistence system.

## Independent source and executable checks

Executed a read-only in-memory assert script via `mise exec -- node --input-type=module` in the UI directory, importing the already-built loaders/plot/Node executor. Source and built bodies agree. It independently confirmed: duplicate-type stat shape incorrectly populated; five distinct series folded to four; explicit panel bounds displaced by inherited bounds; abort during deferred stream startup yielded success, one collect and zero closes. Bifrost.connect was temporarily replaced only in that isolated process and restored in finally. No source or fixture changed.

The signed-geometry proof is direct arithmetic in the complete renderer: niceCeil(-5) is NaN and a negative bar value yields a negative height. The same script checked that arithmetic. It did not exercise browser SVG layout; zero-only time series are not claimed broken (their fallback ceiling is 1). CardHeader action and canonical-error conclusions follow complete producer-to-consumer source traces below.

## Final deduplicated finding ledger

All UI-relative paths below are rooted at `crates/wyrd/wyrd-server/wyrd-ui/`.

### FIND-003-1 — explicit panel range is ignored

**Status:** CONFIRMED. **Discovery:** INV-003-1. **Classification:** INCORRECT.

**Obligation:** REQ-030 says panels inherit document range unless their input sets one; exact executed binds must be truthful. **Location:** `packages/ui/src/lib/loaders.ts:160`.

ChartInputSchema accepts panel ranges. WyrdView.start resolves document/brush bounds and always passes them to its PanelLoader. Both remote endpoints forward those bounds to loadPanel. loadChart chooses `context.range ?? input.range`, so the panel's explicit absolute or relative range loses in every document host. Direct loadChart callers without context range behave differently. The resulting executed binds feed both the query and Open in Query; this is wrong query data, not merely a label problem. Direct example SSR also supplies inherited range and reaches the same cause.

**Correction:** make the existing shared loadChart honor an explicitly supplied input range before the inherited context, resolving a relative input with the existing resolver. Keep once-resolved document bounds for panels without an override, SQL unchanged, and brush/reset reloading every panel. A brush changes the inherited document range; it need not erase the explicit override promised by the same spec. Do not add separate host range policies or change the document contract.

**Focused closure:** existing loader/view tests with two panels, one explicit and one inherited; assert exact query/executed binds initially, after brush and after reset. Pin time for a relative override case. No live query suite is needed to prove this precedence correction.

### FIND-003-2 — validation checks a different duplicate column from rendering

**Status:** CONFIRMED. **Discovery:** B-002, INV-003-2, MNT-003-01. **Classification:** INCORRECT.

**Obligation:** REQ-030 positional duplicate-name preservation and honest incompatible-schema errors; REQ-025 typed chart shapes. **Location:** `packages/ui/src/lib/loaders.ts:127-131`; consumers `plot.ts:18-19,32-46,63-69`, `components/Chart.svelte:153-158`.

Both executors preserve duplicate descriptors and positional rows. shapeChart constructs a name Map (last match), but columnIndex selects the first. Full timeSeries, barRows and explicit stat extraction use that first match for all named bindings. With `[v:Utf8,v:Int64]` and `['bad',3]`, validation says populated and rendering consumes `bad`; reversed descriptor order wrongly rejects a renderable numeric first binding. The shared table path correctly renders every positional cell and must remain unchanged.

**Correction:** select descriptors for shape validation with the existing first-match columnIndex rule. This fixes the producer that incorrectly admits/refuses data; do not add NaN guards at every renderer, rename/deduplicate columns, change SQL, or reject all duplicates. This is necessary reuse of an existing rule, not a new abstraction.

**Focused closure:** duplicate descriptors with conflicting types in both orders, using stat and representative timeseries/bar bindings. Verify incompatible first binding becomes the existing panel error, compatible first binding renders its value, and tables still preserve both positions. Test x/series roles through the same selector where applicable.

### FIND-003-3 — signed chart domains produce invalid SVG geometry

**Status:** REVISED (combined shared signed-domain correction, including the independently reported timeseries path). **Discovery:** B-003, INV-003-3. **Classification:** INCORRECT.

**Obligation:** REQ-025 numeric timeseries/bar shapes impose no nonnegative restriction; Chart is completed here for all hosts. **Location:** `packages/ui/src/lib/components/Chart.svelte:44-70,118-149` and bar/line SVG consumers.

shapeChart accepts signed Int/Float/Decimal data; timeSeries/barRows preserve it. For timeseries -10/-5, bounds.y1 calls niceCeil(-5), whose log10 is NaN; the resulting axes, line and marker positions are invalid. Flat negative data also reaches a nonpositive ceiling. For bars, `value/barMax` produces negative heights for negative values; mixed or entirely negative data therefore omits valid bars. Null/zero handling and label thinning are adjacent existing behavior, not permission to reject signed inputs.

**Correction:** retain Chart's geometry owner and derive finite increasing signed domains; for bars include zero and draw each signed value from the zero baseline with nonnegative dimensions. Keep the current positive-only appearance, null semantics, source/actions, labels, dense fitting and container sizing. No chart dependency or generalized scale framework is needed. Source geometry is the correct boundary; downstream SVG guards would hide valid data.

**Focused closure:** mounted Chart assertions for entirely negative, mixed-sign, flat-negative and zero series/bars, checking finite axis/path coordinates, nonnegative rect sizes and the correct side of zero. Retain existing positive/dense chart checks. A local browser spot-check is appropriate for geometry; this does not require repeating whole host journeys.

### FIND-003-4 — series collapse before the approved threshold

**Status:** CONFIRMED. **Discovery:** B-001, INV-003-4. **Classification:** INCORRECT.

**Obligation:** REQ-025 explicitly folds more than 20 series. **Location:** `packages/ui/src/lib/plot.ts:14-15,47-54`; Chart's STYLES and both indexed style consumers; scenario-1 five-series assertion in `src/lib/ui-library.test.ts`.

timeSeries groups normalized rows into named series, then folds beyond MAX_SERIES=4, keeping three plus other. All Chart hosts consume these lost identities; five through twenty series therefore lose independent data. The four-entry STYLES array explains the implementation but cannot supersede the approved threshold. The test deliberately expecting five to become four is conflicting proof.

**Correction:** apply the approved twenty-series boundary in the existing timeSeries owner and make Chart's existing style selection valid for every returned series, preserving label/tooltips and the approved non-color distinguishing cues. Keep deterministic grouping, totals and ordering. Do not only increase the constant and leave `STYLES[index]` undefined; do not create another series cap or add a plotting framework. This removes the premature restriction rather than adding optional functionality.

**Focused closure:** 5 and 20 distinct series remain individually addressable; 21 triggers other under the approved folding rule. Mount Chart at the boundary so legend/path/tooltip style access cannot crash. Retain sum-per-instant behavior after folding. Fix the conflicting expectation rather than weakening the test.

### FIND-003-5 — CardHeader version/tab controls discard completion state

**Status:** CONFIRMED. **Discovery:** B-004, INV-003-5. **Classification:** MISSING.

**Obligation:** REQ-030 action pending prevents duplicates, refusal/errors stay visible, unavailable actions give a reason; TASK-003 requires declared action results. **Location:** `packages/ui/src/lib/components/CardHeader.svelte:55-60,103-113`.

The schemas declare card.version/card.tab host-navigation actions. WyrdView.act validates and records outcomes in panel.actions, then supplies that map to CardHeader. Direct component hosts also supply ActionProps. CardHeader reads only the Observe action entry; the select and tab buttons ignore their pending/denied/error/unavailable states. WyrdView suppresses duplicate host dispatch centrally, but the controls show no outcome and direct hosts still emit duplicates. A native select may display a refused version while the unchanged Card remains authoritative. Gallery and direct custom consumers reach the same component; this is not limited to missing future navigation routes.

**Correction:** have these controls consume their existing named ActionState, using ActionButton's established pending/feedback convention while preserving native select/tab semantics. Disable pending/unavailable interactions, show refusal/stable errors/unavailable reasons and supplied success feedback, and keep visible selected identity/tab truthful to props after refusal. No new dispatcher, route or server action is required. A consumer guard belongs here because the component owns its controls and every direct host consumes that contract.

**Focused closure:** direct CardHeader pending prevents version/tab callbacks; denied/error/unavailable are visibly reported; successful host update reflects authoritative props. Also pass one outcome through WyrdView to prove delivery, retaining exact emitted references and tab IDs. Do not demand a new production navigation destination in this remediation.

### FIND-003-6 — abort during query startup misses supported cancellation

**Status:** CONFIRMED. **Discovery:** B-005, INV-003-6, SYS-003-1. **Classification:** INCORRECT.

**Obligation:** REQ-030 propagates supersession/cancellation to supported executor work; no usable result after cancellation. **Location:** `packages/ui/src/lib/node.ts:29-36,49-57`.

run checks signal before read, which then awaits Bifrost connection and SDK stream startup. Bifrost.stream awaits native query start before handing out the stream (`sdks/wyrd-sdk-ts/wyrd/src/index.ts:1287-1315`). An abort during either await is not replayed to the listener attached later. collect then succeeds; the success branch has no abort check. SDK BifrostQueryStream.return already calls native close, whose cancellation owner interrupts collection. WyrdView's current-generation guard hides stale rows but does not release that work. Both BFF and custom view endpoint forward request signals; direct executors can also return the wrong success. MCP separately forwards the signal to its host and is not this defect. Other card/list/status reads offer no equivalent stream-close API and do not need a new transport cancellation scheme.

**Correction:** at this existing adapter's acquisition boundary, close a stream acquired after signal cancellation and refuse to collect/return success; retain the active-collection listener and cleanup, and ensure cancellation winning before completion is not returned as usable rows. Use stream.return; do not cancel unrelated reads, add retries, or move lifecycle into the renderer. The guard belongs here because only this adapter owns the acquired stream and signal together.

**Focused closure:** deterministic deferred stream-start test: start with live signal, abort while acquiring, release stream, assert return once, no collect and no success. Also assert cancellation during collection and existing pre-aborted behavior. A small adapter test closes the race; the historical server journey remains transport proof and need not be repeated wholesale.

### FIND-003-7 — public UI errors bypass the canonical catalog/projection

**Status:** REVISED (retain canonical-contract defect; constrain correction to safe projections and avoid broad error-framework changes). **Discovery:** REP-003-01. **Classification:** VIOLATION.

**Obligation:** AGENTS §§4/9 and errors guidance prohibit TypeScript-only public Wyrd codes and handwritten parallel HTTP problem payloads. **Locations:** `packages/ui/src/lib/loaders.ts:105,119,234-242,280`; `mcp.ts:24-29`; `view.ts:63`; `WyrdView.svelte:69,201,237`; first-party `src/routes/t/[tenantKey]/ui/load/+server.ts:33-34`; `examples/ui-app/src/routes/view/load/+server.ts:15-16`.

Local missing-range/shape errors travel through exported loaders into Loaded/EmptyState. Missing executor, rejected load/action, remote non-problem error and unstructured MCP failure have sibling producers emitting WYRD_UI_* identities. These are absent from wyrd-spec's canonical catalog and generated error-code union. They are publicly displayed as stable Wyrd errors, so they are not merely private log messages. Both endpoints also serialize the reduced four-field Problem for malformed panel requests, dropping canonical type/details/remediation. Existing node/MCP upstream projections preserve code/title; this finding does not require all display props to become full HTTP problems or reclassify server refusals.

Existing owners: `wyrd-spec/src/error.rs` Validation/Internal and other applicable boundary variants; generated `wyrd-spec/schemas/ui_problem_examples.json`; first-party `$lib/server/problem.ts::problem/safeProblem`. Full bodies confirm these are browser-safe metadata projections, not another credential-bearing client. The package cannot runtime-import a first-party `$lib` server module; publish the needed projection from the existing generated source under the current package/build owner. Do not hand-copy a new list or runtime-import the native SDK into browser exports.

**Correction:** replace local UI-only identities with existing canonical generated validation/internal or appropriate upstream projections. Carry safe component/field diagnostic context with the existing detail mechanism. Reuse generated canonical metadata at both HTTP boundaries, including type/details/remediation, while preserving canonical upstream identities and the current denied/error mapping. Keep internal exception strings out of newly exposed HTTP diagnostics; this is preservation of the existing safe boundary rule, not a new security redesign. No new backend error domain, route or transport is required.

**Focused closure:** missing range, incompatible shape, absent/failed host loading/action and MCP fallback use catalog-backed code/title. Malformed-panel HTTP response includes canonical validation metadata and safe field detail. Known upstream permission errors retain their exact identity and denied state. Browser package exports stay native-free. Run codegen:check only if generator/output wiring changes, plus existing package checks/build and focused affected UI tests.

## Proposal reconciliation and acceptance/proof limits

Every discovery proposal is accounted for in the seven entries above; duplicates are consolidated, not separate work. No proposal is rejected outright. B-003's bar-only description is expanded by the independently validated timeseries path; REP-003-01's broad correction is narrowed to generated metadata reuse and safe boundaries, not a new public error architecture. No optional style notes are converted into findings. No prior-finding closure exists in this first attempt.

| Obligation group | Reconciled assessment |
|---|---|
| Exact inherited/explicit range and SQL reproduction | FAIL, FIND-003-1; SQL itself remains unchanged |
| Positional normalization and truthful chart binding | Normalization/table preservation supported; named chart binding FAIL, FIND-003-2 |
| Complete numeric charts and approved cardinality | FAIL, FIND-003-3/4 |
| Card lookup/listing identity and action outcomes | Read/dependency/paging paths supported; version/tab outcomes FAIL, FIND-003-5 |
| Supersession and supported executor cancellation | Stale render isolation supported; acquisition race FAIL, FIND-003-6 |
| Canonical public errors | FAIL, FIND-003-7 |
| Browser/native separation, catalog authority, four-host foundation, request-scoped session tenancy, query-only Rust construction | Discovery evidence and inspected source support these; no additional material finding confirmed |
| Human visuals and non-goals | Supplied approval honored; no persistence/Ask/A2UI/production MCP-registration/publishing scope added |
| Linux native/image closure and live degraded terminal parity | Remain UNVERIFIED, never PASS based on release wiring alone |

The orchestrator's freshly rerun seven UI tests, exact scenario selectors, Svelte checks/build and diff check are credible within their assertions. Historical live SDK/MCP/BFF and Rust results are recorded evidence, not checks rerun by this validator. The live cancellation assertion uses a pre-aborted signal and misses FIND-003-6. Existing tests also omit the retained binding, signed-domain and action-state edges; one test expressly pins the wrong series threshold. Shared JsonResultBuilder supplies meaningful degraded-projection source evidence but not a live degraded Node/MCP parity run. Linux bundle/image completion evidence is expressly absent and the task says it must remain unverified; do not claim task-wide platform closure. These proof gaps do not establish a speculative Linux implementation defect. Obtain the outstanding platform/degraded evidence at its owning verification stage before declaring those obligations fully proven; do not put broad journey/release aggregates into the bounded remediation.

Skipped: fresh live infrastructure journeys, Rust aggregates, Linux bundles/images and new visual inspection. Risk: seven confirmed behavior/contract gaps remain, and platform/degraded proof is incomplete.
