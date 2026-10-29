# TASK-027 independent system-resilience review

Overall result: **PASS**. No material proposed finding. FIND-027-6 is closed at the narrow presentation join without changing read-generation ownership or widening the failure boundary.

## Immutable subject and scope

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`.
- Cumulative base: `5e57416d562386066c1aadda9dd01783b9cd4eb5`.
- Candidate and checked HEAD: `39c15176955edca785cc578168c67e1efcb80ea5`.
- Approved specification: `changes/active/wyrd-ui-main-integration/spec.md`, revision 13; original task `tasks/TASK-027-view-layout.md`.
- Remediation locator: prior packet `task-027-7579a2f60-1/TASK-027-R4-narrow-document-replacement.md`, implementation `3302a3e6b`, evidence `39c151769`.

Reviewed cumulative runtime/source changes and tests, not just R4. Independent TASK-003 changes and flat Card identity are inspected as adjoining loader/component/transport seams, not attributed to TASK-027 unless they break layout. No current-round discovery report was consulted. Prior validation and remediation supply hypotheses; closure below is based on current source.

Read root AGENTS.md, specification requirements REQ-030/036 and AC-015, original task and R4, prior finding ledger, architecture authority map and `wyrd-design.md` client-model boundary, spec-driven-development, maintainer style, and applicable TESTING.md guidance. Shared subject reports no nested AGENTS.md in the changed UI/example paths. There is no root `.codegraph`, so CodeGraph does not apply. TASK-027 does not modify operational server topology, database durability, authorization, or tenant coordination.

UI-relative locations below start at `crates/wyrd/wyrd-server/wyrd-ui/`.

## Deployment and lifecycle trace

| Deployed path | Actual owner and caller evidence | State and blast radius |
|---|---|---|
| Custom Svelte host | `packages/ui/src/lib/view.ts` exports WyrdView directly; renderer receives doc/executor/load/onlayout; gallery and example `/view` pass reactive document state | Geometry, retained edits, measurements and drafts belong to this component instance. No remote write or persistent layout is introduced |
| DOM mounting | `view.ts::mountView` copies options into Svelte mount and exposes destroy through unmount | Same renderer and lifecycle, no competing loader/gesture store |
| MCP App iframe | `packages/ui/src/app/main.ts:23-28` destroys previous view per tool result, mounts a new one with the existing executor and a layout callback | Completed edits remain in renderer memory for one render. Next result/reload starts authored geometry. No edits go through tools/call |
| Loading | `WyrdView.svelte:64-89` derives source/loader and the layout-stripped content key; `start`, `abortAll`, `settle` at 235-267 retain load-generation ownership | Content/range/source changes start ordinary new reads. Layout-only changes do not reload or reset brushed range |
| Remote reads | `view.ts::remoteLoader` supplies AbortSignal to fetch; `mcp.ts::mcpExecutor` passes it through callServerTool; `node.ts::nodeExecutor` closes an active query stream on abort and checks cancellation before accepting a collected result | Existing executor/transport ownership remains. Client cancellation may not stop every remote operation; renderer settlement independently rejects stale results |
| Child rendering | Renderer `.fill` contains Chart/CardHeader; fixed row pitch and measured intrinsic occupancy come from `layout.ts` and height binding | Panel content/error is local. Layout does not introduce server-health coupling, background retries, timers, persistent queues, or an availability dependency |

The changed renderer remains a presentation owner under the architectural server/client boundary. Existing `loadPanel`, load-generation control, native Svelte dimension binding, and platform pointer capture are reused. Pure placement/edit geometry needs no second lifecycle framework. R4 reuses the placement Map at the only join that combines current authored geometry with effect-owned loaded rows.

## Failure, interruption and recovery assessment

| Credible event | Source-backed behavior and affected capability | Recovery and proof |
|---|---|---|
| Narrow valid document replacement with different IDs while old reads are pending | Current placements update before `start` replaces loaded rows. `ordered` at 142 filters loaded IDs by current Map membership before sorting. Obsolete IDs cannot reach the comparator; no coordinates are fabricated | Existing post-render load effect creates current loading rows. Current IDs include loading, failed and validation-error states because membership is independent of props/result. Narrow scenario at test lines 1043-1088 covers a/b -> c/d, reduction, failure/loading visibility and invalid -> valid recovery |
| Old request completes after replacement | Cleanup/start abort old controllers; `settle` requires both non-aborted signal and membership in current controllers before finding current row | Late old data cannot resurrect removed panels or overwrite current same-ID input. Narrow test resolves old pending request and checks absence/loading; existing dependent-load test at 415-425 also proves wide same-ID replacement |
| Read refusal, dependency outage or rejected Promise | `start` rejected-Promise handler supplies a failed canonical problem through `settle`; successful load returning Problem is also converted to failed. Invalid panel input creates an error row without starting IO | Failure stays within that panel; siblings render, editing remains local, no automatic retry storm or shared server crash is introduced. Current narrow c failure remains visible beside d loading. Subsequent content/range/document replacement uses existing start path |
| Layout change/host echoes callback during an outstanding load | `content` excludes only layout while full authored value/reference reset owns draft/kept lifetime; stable source prevents default loader recreation for the same executor | Existing reads continue; unchanged brushed range is retained. Tests verify layout-only reordering/read count and completed host echo without new loads |
| Pointercancel or Escape during move/resize | `cancel` calls release and clears draft; `drag::stop` removes element pointer listeners and window Escape listener and releases capture when held | Completed geometry survives, preview disappears, later movement/up is inert, fresh gesture works. Pointer cancellation scenario covers these terminals |
| Blur/Enter finishes a held gesture before pointerup | `commit` releases before checking draft and reporting; same cleanup owns both keyboard completion and pointer terminal path | One report with all kept edits, no later producer can revive draft. Existing tests exercise focused grip and unfocused corner paths and no-movement completion |
| View narrows, host removes handler, or authored doc changes mid-gesture | Editability and authored/given pre-effects cancel before controls disappear or document values replace local geometry | No stray completion after lost affordance; completed sibling edits remain until a new authored document supersedes them. Existing reactive-proxy and handler/width transition tests cover this |
| Component teardown / next MCP result / browser reload | Effect teardown releases held gesture; load effect cleanup aborts outstanding controllers; MCP handler destroys old instance | Old reads cannot settle; new component resolves fresh reads and authored layout. Browser-memory edits are intentionally lost, with no durable data to recover or migrate |

No task-attributed path newly turns a panel failure into a shared process/service exit. No new fail-closed process recommendation is warranted. Integrity checks remain at the document, panel validation and read-generation boundaries; availability is preserved for unaffected sibling panels and unrelated server capabilities.

## Cumulative obligations and prior-finding closure

| Obligation / prior finding | Current evidence | Result |
|---|---|---|
| Bounded valid layouts; no mutation by collision resolution | LayoutSchema validates bounds; place copies supplied layouts and displaces later boxes; boxStyle reads only resolved geometry | PASS |
| Layout effects stay local and do not add remote reload/persistence | `content`, source/loader derivation, commit callback, MCP render lifecycle, gallery/example memory-only callbacks | PASS |
| FIND-027-1: callbacks preserve earlier non-echo sibling edits | commit builds done from all kept entries and maps the full parsed document with own-property checks; corresponding layout-editing test preserves untouched auto/colliding siblings | CLOSED |
| FIND-027-2: every gesture terminal disables the producer | release-before-return commit, cancel, stop listener/capture cleanup, authored/editability effects and teardown; pointer cancellation test covers blur, Enter, Escape, pointercancel, handler removal and narrowing | CLOSED |
| FIND-027-3: intrinsic content cannot silently consume wrong fixed rows | rowsFor/place reserve measured occupancy; auto panel retains intrinsic height; fixed grid rows and rowsHeight use 40px rows/12px gaps; measurement-change tests prove displacement | CLOSED; physical geometry evidence remains historical |
| FIND-027-4: arbitrary accepted panel IDs work across state writers/readers | own-entry guards on kept/heights, immutable computed-property writes, Map placements; special-ID geometry/edit tests include constructor, toString and __proto__ with changing measurements | CLOSED |
| FIND-027-5: reactive authored updates supersede local edits/gesture | full authored serialization plus given identity trigger cancel/kept reset independently of load content; native proxy tests cover layout, range and input writes | CLOSED |
| FIND-027-6: narrow replacement must not dereference obsolete geometry | ordered filters against placements before sorting; stable sort on copied filtered array retains current row order for coordinate ties; start/settle unchanged | CLOSED |
| Narrow layout and failure states remain usable | Existing narrow test now proves current order/full-width/height/read-only, failure/loading, removed IDs, late settlement, invalid recovery, changed input reload and layout-only no-read behavior | PASS |

The R4 guard belongs at the projection because each authority is individually valid and their join creates the invalid comparator domain. Moving IO into a pre-effect, adding generations/stores, forcing host remounts or restricting cross-document IDs would broaden the correction unnecessarily. Existing read-generation ownership still handles aborted/stale work.

## Verification assessment and limits

Fresh work here is independent static inspection of the cumulative source/diff, current callers, complete changed renderer and geometry workflow, R4 diff and persisted tests. No checks were rerun or runtime probes claimed. Caller supplies candidate-green format/lints/codegen, TypeScript native/unit checks, queue tests, UI package/app check, 40/40 UI tests, build and real-server UI integration 2/2. R4 records the pre-fix TypeError RED and focused GREEN; the persisted test directly exercises the diagnosed transition rather than masking timing through a sleep/retry.

Existing source/test evidence credibly establishes cancellation/replacement recovery for this local change. Historical browser gallery/AppBridge geometry, focus/capture, themes and human acceptance remain the physical-browser evidence; jsdom dimensions and simulated events do not independently establish those properties. No fresh browser, process-restart/load test, deployment or production availability qualification is claimed. The task does not change a server restart/recovery path requiring such new proof.

Active proposed findings: **none**. No remediation recommendation.

Skipped redundant integrated suites and fresh browser execution. Remaining qualification limit: physical geometry/capture relies on historical browser evidence; no new system-resilience defect is established.
