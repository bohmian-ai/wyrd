# TASK-003 independent structured Ponytail validation

Two distinct material findings are retained. The original causes of FIND-003-1 through FIND-003-7 are closed; the two current defects do not reopen those diagnoses. This report supplies the validated ledger and correction boundaries, not a task verdict.

## Subject and method

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Base: `695ca914df61a8683059f6f32279ec2148424e06`.
Immutable candidate: `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`; independently checked HEAD before inspection and before report completion.
Specification: `changes/active/wyrd-ui-main-integration/spec.md`, revision 12.
Original task: `tasks/TASK-003-shared-ui-foundation.md` within that packet.

Read this attempt's subject, verification, claims comparison, both implementation reports, standards, maintainer, system, security and focused follow-up reports. Read the prior verdict, seven-finding validation ledger and R1 task with appended execution evidence. Independently checked the actual cumulative owners and remediation diff; discovery agreement was not treated as proof. Applied AGENTS.md, agent-rules, spec-driven-development, maintainer and error guidance, Wyrd design/doctrine boundaries, the UI skill, and approved REQ-025/030/033. No CodeGraph index exists. Separately approved TASK-027 layout behavior and the subject's excluded/upstream changes are not foundation findings.

Read the full bodies at both proposed correction boundaries (`contract.ts` and `timeSeries`) and expanded their complete runtime consumer graphs. Also checked the prior correction owners: loadChart/chartParams/shapeChart, columnIndex/barRows/stat extraction, Chart geometry and rendering, CardHeader/ActionButton/ActionNote, nodeExecutor/read/run and SDK stream lifecycle, inputProblem/validatePanel/loadPanel/validateIntent, remoteLoader, WyrdView loading/action lifecycle and both panel endpoints. Package exports, generator, emitted ESM, app/gallery/custom hosts, shared-client query-only construction, bundle changes and actual regression assertions were used to assess closure and adjacent behavior.

UI-relative paths below start at `crates/wyrd/wyrd-server/wyrd-ui/`.

## Original-ID closure evidence

| Original ID | Independent owner/caller/sibling evidence | Closure and proof assessment |
|---|---|---|
| FIND-003-1 | `loaders.ts:156-162` selects and resolves `input.range` first, falling back to the already resolved context. WyrdView.start supplies inherited/brush bounds to this same loader; both remote endpoints pass context rather than overriding panel input; direct custom SSR uses the same owner. `executed` and the executor request use the same params. | CLOSED. Existing dependent-load test checks two panels before/after brush and reset and a fixed-clock relative override. No second host range policy was introduced. |
| FIND-003-2 | `shapeChart.need` at `loaders.ts:125-129` selects through `columnIndex`, which uses first-match `findIndex`. timeSeries, barRows and explicit stat extraction select the same position. Table header/cell loops retain positional duplicates. | CLOSED. Rendering tests check conflicting descriptor orders, stat output and x/series roles, with positional table assertions. No duplicate renaming, schema rejection or per-renderer guard is necessary. |
| FIND-003-3 | Chart's `bounds` at `components/Chart.svelte:46-56` includes zero, rounds each signed side outward and gives zero-only data span 1. `barDomain`, `barY` and `barCells` at lines 123-163 map signed endpoints around zero to nonnegative heights; negative value labels reserve space before category labels. The same Chart serves direct and view hosts. | CLOSED for the diagnosed signed-domain failure. Mounted assertions cover mixed, negative, flat-negative, zero and positive cases; supplied browser evidence addresses labels/geometry. Existing source values/null semantics/actions remain. Prior reviewer RED is recorded honestly; another artificial RED round is not a correction. |
| FIND-003-4 | `plot.ts:15,48-54` folds only beyond twenty, retaining nineteen plus the sum; Chart.style at line 37 has twenty distinct marker/dash pairs, reused by plot and legend. At or below twenty there is no folding. | CLOSED for the original premature threshold and unsafe style-index causes. Existing 5/20/21 mount/focus assertions prove threshold, style coverage and sum 3. The authored-label collision below is a different cause requiring different input and correction. |
| FIND-003-5 | `CardHeader.svelte:36-40,63-80,117-125` consumes version/tab states, disables pending/unavailable controls, resets the native selection to authoritative input and renders shared ActionNote. ActionButton consumes that same note. WyrdView.act still validates and suppresses pending duplicate dispatch, then returns outcomes through panel.actions. | CLOSED. Direct state/selection checks and one WyrdView-delivered refusal cover the missing behavior. Component control guards belong here because direct hosts also consume them; no dispatcher or destination was added. |
| FIND-003-6 | `node.ts:30-43` owns stream and signal together: abort during acquisition invokes stream.return before collect, active abort invokes the same existing close, post-collect abort cannot return rows, and finally removes the listener. SDK `BifrostQueryStream.return` at `sdks/wyrd-sdk-ts/wyrd/src/index.ts:559` calls native.close; collect is the existing bounded owner. Both endpoints forward request.signal, direct Node callers receive the same behavior, and WyrdView additionally rejects stale generations. Other reads have no new cancellation API to duplicate. | CLOSED. The deterministic test covers acquisition, active collection/late result and pre-aborted input. This is stronger than the historical pre-abort-only journey. No retry, shared-process cancellation or unrelated read lifecycle moved. |
| FIND-003-7 | `scripts/catalog.ts:29-34` distributes the existing Rust-generated safe problem JSON; `contract.ts:41-42` projects it with safe context. chartParams, shapeError, inputProblem/intent validation, MCP fallback, remoteLoader and WyrdView load/action fallbacks use it. Both endpoint refuse functions serialize that complete object with problem content type; unexpected executor errors still go through the existing first-party safe server boundary. Node/MCP upstream code/title and failed() denial mapping are preserved. | CLOSED for the canonical identity and reduced HTTP-payload causes. Canonical tests assert local metadata, both HTTP responses, upstream refusal preservation and browser-native separation. FIND-003-8 is a new module-linking regression in this remediation, not another uncatalogued-code finding. |

Closures describe implemented source and the supplied focused proof. They do not imply that every original acceptance edge is now covered: the separate defects below remain reachable.

## Proposal reconciliation

| Discovery proposal | Validation | Stable disposition |
|---|---|---|
| B-003-9; INV-003-8: native Node cannot link public loaders | CONFIRMED independently through both public export specifiers and emitted source | FIND-003-8 |
| B-003-8: authored `other` collides with the synthetic aggregate | CONFIRMED consequence; REVISED recommendation to reuse only the existing caption field, without a new identity contract | FIND-003-9 |
| MNT-003-08: same collision, addressed by unkeyed/index loops alone | REVISED into FIND-003-9; duplicate diagnosis consolidated. REJECTED correction-only alternative: fixing reconciliation alone leaves indistinguishable authored/aggregate captions in tooltip and accessible name. | FIND-003-9, no extra finding |
| Follow-up's single producer caption correction | CONFIRMED independently from all sibling consumers and authority | Included in FIND-003-9 |
| Other discovery reports | No material findings proposed; source-backed closure and scoped PASS claims assessed above | No additional finding |

Neither retained finding is a remaining instance of an old cause. FIND-003-8 arises from the new R1 runtime JSON import; the canonical metadata itself is now correct. FIND-003-9 is present in the earlier folding implementation as well, but the prior finding concerned folding at five and style coverage, not conflicting authored/synthetic captions. Changing the threshold does not create or repair this collision. Both are genuinely distinct diagnoses; new stable IDs start after 7. No finding is assigned to a human-directed addition, an unexecuted Linux lane or a direct-cwd example invocation.

## Final deduplicated ledger

### FIND-003-8 — published runtime JSON import cannot link in native Node

**Status:** CONFIRMED. **Sources:** B-003-9, INV-003-8. **Class:** REGRESSION.

**Obligation:** original TASK-003 requires approved exports to work from package contents; REQ-030 requires portable loaders usable on the user's Node server. **Location:** `packages/ui/src/lib/contract.ts:19`, emitted unchanged as the JSON import in `packages/ui/dist/contract.js`.

**Producer-to-consumer proof:** the existing catalog generator copies `crates/wyrd-spec/schemas/ui_problem_examples.json` into source `problems.json`. That data authority is appropriate and the generated file is included in dist. svelte-package emits the ordinary ESM import with no JSON import attribute; package.json declares `type: module` and maps public `./loaders` to dist/loaders.js and `./mcp` to dist/mcp.js. Both modules import runtime functions from contract.js, so linking fails before any loader/tool call or local error path executes. view.ts and WyrdView share those runtime helpers under their existing Svelte/bundler hosts. `/node` imports contract types only, so a successful SDK/Node-adapter import does not cover this broken sibling graph. There is one runtime import of this JSON in the portable source graph; fix it once.

**Independent executable evidence:** from the UI workspace, through pinned mise Node, imported each public specifier in an isolated read-only process:

```text
wyrd-ai-ui/loaders ERR_IMPORT_ATTRIBUTE_MISSING .../packages/ui/dist/problems.json needs an import attribute of "type: json"
wyrd-ai-ui/mcp ERR_IMPORT_ATTRIBUTE_MISSING .../packages/ui/dist/problems.json needs an import attribute of "type: json"
```

Source and built import agree. This is successful package resolution followed by failure inside valid package contents, unrelated to the example's supported workspace command. Vite builds and Vitest transform JSON and therefore do not falsify this result.

**Smallest complete correction:** retain the existing generated JSON, catalog generator, canonical helper and native-free browser graph. Use the standard ESM JSON import attribute (`with { type: 'json' }`) on this existing metadata import. Deletion would remove required canonical errors; a handwritten TS catalog, bundling layer, custom loader, filesystem read, Node-only helper or dependency would duplicate existing/native behavior. No public contract, credential, query, tenant, resource or server decision is needed.

**Focused closure:** after the normal package build, plain pinned Node must import both public exports and invoke a canonical local-error path. A compact assert-based command from the existing UI workspace is sufficient; integrate that check into an existing package verification owner rather than create another harness:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui package
mise exec -- node --input-type=module -e 'import assert from "node:assert/strict"; const loaders = await import("wyrd-ai-ui/loaders"); const mcp = await import("wyrd-ai-ui/mcp"); assert.equal(loaders.chartParams("SELECT $1", undefined).code, "WYRD_SPEC_400_VALIDATION"); assert.equal(typeof mcp.mcpExecutor, "function");'
```

Run the Node command with cwd `crates/wyrd/wyrd-server/wyrd-ui`. The corrected package must also retain the existing canonical browser test and UI check/build compatibility. No server, native SDK rebuild or live journey is needed to prove module linking.

### FIND-003-9 — valid authored series caption collides with overflow aggregate

**Status:** REVISED (one shared producer correction). **Sources:** B-003-8, MNT-003-08 and focused follow-up. **Class:** INCORRECT.

**Obligation:** REQ-025 accepts an optional text series column and folds overflow into “other”; REQ-030 requires complete Chart rendering through its supported hosts. No authored caption is reserved. **Location:** `packages/ui/src/lib/plot.ts:48-54`; sibling consumers `components/Chart.svelte:209,233,256,263,270`.

**Producer-to-consumer proof:** shapeChart accepts every UTF-8 series caption. timeSeries groups authored series into a Map keyed by caption, ranks totals, keeps nineteen and appends a separate `['other', other]`. If authored `other` ranks in those nineteen, two distinct values get the same caption. Chart alone calls timeSeries in production; its plot, tooltip and legend key all three iterations by line.label. Installed Svelte's each-key validation (`node_modules/svelte/src/internal/client/dom/blocks/each.js:758-781`) explicitly rejects duplicates, and reconciliation uses the same key. The accessible name joins captions and the tooltip renders plain caption/value spans; neither identifies which `other` is authored versus aggregated. Legend/plot dash-marker cues distinguish lines visually but do not repair that tooltip attribution.

All data paths converge here: executor-loaded results pass shapeChart, direct hosts may pass the same valid props, WyrdView renders this Chart, gallery/custom pages use it directly, and the MCP artifact bundles it. There is no other aggregation writer, renderer-level normalization or host-specific workaround to extend. No public package export exposes Series/timeSeries as a declared API.

**Independent executable evidence:** imported only the already-built plot module (its contract import is type-only), supplied one timestamp, groups s0..s19 valued 1..20 and authored `other` valued 21. The current owner returned:

```json
{"count":20,"unique":19,"duplicate":[{"label":"other","points":[{"x":1792490400000,"y":21}]},{"label":"other","points":[{"x":1792490400000,"y":3}]}]}
```

The built/source folding bodies agree. This validates reachability with ordinary permitted values; no SQL parser, transport or fixture issue is involved. Existing threshold tests use m1..m21 and cannot detect this caption.

**Smallest complete correction:** in the existing timeSeries overflow producer, qualify the synthetic aggregate caption only when needed so it remains identifiable as the “other” aggregate and differs from every retained authored caption. Reuse Series.label; preserve every authored label and point, deterministic ranking, nineteen-plus-one limit, sum per instant, ordinary non-colliding `other` caption and all at-or-below-twenty behavior. Collision avoidance must handle authored text equal to any proposed alternate caption; changing to one different fixed string is insufficient.

This selects the follow-up's boundary for independently established reasons: one caption correction fixes keyed rendering, tooltip attribution, legend and accessible name together. Removing keys or using positions alone repairs only reconciliation. A new ID field alone also leaves the plain caption/value tooltip ambiguous. A separate aggregate flag plus multiple consumer changes is unnecessary; merging the authored value into the aggregate, rejecting `other`, hiding one line or reserving a text value changes accepted data behavior. This is a reversible private presentation correction, not a new series/public protocol decision.

**Focused closure:** extend the existing `TASK-003: Validated portable rendering` case using its existing mount/focus/rerender support. Mount twenty-one groups with authored `other` retained; assert twenty plot/legend entries, distinct captions, authored value 21 intact and aggregate sum 3 separately attributable in the tooltip and accessible name. Include a retained authored caption equal to the chosen qualification to reject a merely fixed alternate. Rerender with changed ranking/cardinality and assert current captions, points and cues remain correct. Retain the existing 5/20/21 threshold/fold tests. No live server, public schema or new test environment is involved.

## Focused verification recommendations and limits

For the two bounded corrections, reuse the existing exact cases and package owners:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^Canonical UI problem projections$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
```

Add the native Node import/assert check above after building. No codegen change is necessary for an import attribute and a private aggregate caption; run codegen:check only if generated-input wiring actually changes. No new harness, dependency, broad Rust aggregate, platform bundle or whole user-journey rerun belongs in this remediation.

The orchestrator's nine exact focused results, full-file reviewer result, zero-diagnostic UI/example checks, builds and codegen results are credible within their actual assertions; these were not rerun by this validator. I independently ran the native public-export and plot probes above without changing source or building. Historical real SDK/MCP/BFF/custom-host journeys, Rust checks, macOS bundle and supplied human visual approval remain historical evidence. None proves native JSON linking or arbitrary aggregate captions.

Skipped: implementation, broad/redundant test reruns, fresh browser/live-server checks and Linux bundle/image execution. Risk: the two reproduced defects remain; Linux and live degraded-terminal parity remain explicit evidence limits, not inferred defects or verified outcomes.
