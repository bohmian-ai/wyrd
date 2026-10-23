# TASK-027 independent findings validation

## Subject and evidence

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; original base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; immutable candidate and inspected HEAD `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`.

This is the fresh structured Ponytail validation role. Read subject.md, verification.md, claims-comparison.md, both task reviews, standards-review.md, maintainer-review.md, system-review.md, domain-review-interaction.md and followup-review.md. Inspected approved revision 12 REQ-030/036 and AC-015, original TASK-027, prior validated FIND-027-1..3 and verdict, R1 and its execution evidence. Applied supplied AGENTS.md, agent rules, reference routing, spec-driven development, maintainer style, applicable Wyrd design/client and doctrine boundaries, TypeScript guidance, Wyrd UI instructions and Ponytail ladder. No `.codegraph` exists.

Reviewed cumulative TASK-027 source/diff rather than just R1: bounded schema and catalog projection; complete layout geometry; complete renderer, including load/action siblings; mount API; AppBridge bootstrap; gallery/example hosts; and existing focused schema/grid/edit/cancel/narrow tests. Separately owned TASK-003 corrections and unrelated merged Rust/deployment changes are not attributed to this task. No source, test, dependency, fixture or harness was changed. Only this report is produced; it is not a task verdict or remediation task.

UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui` unless qualified.

## Authority and complete ownership trace

REQ-036 requires valid fixed-grid placement, whole-document completion, optional editing, cancellation, and read-only behavior below 720px/without a handler. Original scenario 3 requires one callback per completed change. R1 explicitly preserves Enter/blur completion, pointerup once, pointercancel, earlier retained edits, document immutability and no reloads. These are approved existing outcomes; correcting them requires no new product, API, persistence or cross-service decision.

| State/owner | Every relevant producer/writer | Consumers, callers and existing mechanism |
|---|---|---|
| Valid document | `schemas.ts:197-201,213-226` accepts any nonempty unique panel ID; Layout is strict and bounded | Renderer safeParse; `view.ts:38-41` validateView; generated catalog. Neither schema nor task reserves JavaScript prototype names. |
| Retained authored layouts | `WyrdView.svelte:96` initializes `{}`; parsed-document pre-effect resets at `107`; commit replaces at `152-154` using computed-property spread | `layouts:112-114` and callback overlay `155`. Enter, grip blur and pointerup are all commit callers. No host merge owns this state. |
| Measured height | `WyrdView:116` initializes deep `$state({})`; sole writer is native `bind:offsetHeight={heights[panel.id]}` at `295` | `placements:117-122` passes values to pure `place`; boxStyle/current/ordered consume the resulting Map. Installed Svelte size binding observes border-box changes and tears down its observer; it is the native mechanism to keep. |
| Preview | Keyboard arrows `171` and captured move `194-195` write draft; commit `153`, cancel `161`, pointercancel `208` clear it | layouts, editing outline, placement, current and narrow ordering. Draft takes priority over retained/authored layout even when editable is false. |
| Pointer producer | `drag:185-215` cancels previous interaction, captures pointer and registers target move/up/cancel plus window Escape; `release=stop` | stop removes those four listeners and releases capture. cancel, pointer end, new document and unmount reach cleanup. Commit and loss of editable do not. Both grip and corner call this same drag owner. |
| Edit availability | width binding `275` → narrow `93`; callback prop → editable `94` | `297-307,320-322` remove both editing controls when unavailable. No existing effect cancels preview/producer for this transition. |

All full functions that the corrections affect were read: commit, cancel, key, drag (including move/stop/end/escape closures), parsed-document pre-effect, boxStyle/current, layout place/edit, mountView, and each production host. Loader/start/settle/abort/action bodies were inspected to preserve adjacent lifetimes. Existing `loaders.ts:247,274` uses `Object.hasOwn` for ordinary dictionaries; placements already uses native Map. Chart's range brush is a separate interaction, not an alternative layout lifecycle. There is no reusable layout store or second renderer to extend.

`view.ts:44-47` forwards onlayout without requiring feedback and exposes destroy. `src/app/main.ts:22-27` destroys old renders and supplies an intentionally non-echoing callback. Gallery `src/routes/_gallery/+page.svelte:102` and repository `examples/ui-app/src/routes/view/+page.svelte:11` echo documents. Every host reaches the same renderer; host feedback can mask unfinished pointer cleanup but cannot be required. Layout-stripped content and stable source/loader at renderer `61-89` keep reads independent of layout-only edits. Those boundaries need no change.

## Prior-finding closure

| Stable ID | Independent disposition at candidate |
|---|---|
| FIND-027-1 | **Closed for its original cause.** commit now serializes all retained completed edits through done, instead of only the latest panel. Existing non-echo a/b test checks callback two, untouched unplaced/colliding siblings, immutability, feedback and no reload. New inherited-key corruption below has a distinct producer and is FIND-027-4; it does not reopen the lost-sibling diagnosis. |
| FIND-027-2 | **Original Escape reproduction closed; retained as REVISED for incomplete common gesture lifetime.** cancel now ends release before clearing draft, and stop removes actual move/up/cancel/window-key listeners and capture. Focused/unfocused move and corner Escape evidence is credible. Completion and editability-loss still omit that same owner; two interaction proposals consolidate here. This does not claim the Escape repair failed. |
| FIND-027-3 | **Closed for its original cause.** measured offsetHeight feeds rowsFor/place; tracks stay 40px; auto boxes keep intrinsic height with align-self:start; current reads the same occupancy; callbacks omit render-only geometry. Pure/DOM assertions plus supplied physical short-header y4/top208, tall-chart pushdown/top312, settling/width/mobile evidence address the prior stretched-track cause. Arbitrary-ID measurement writes belong to FIND-027-4, not another geometry finding. |

## Proposal dispositions and deduplication

| Discovery proposal/claim | Disposition | Deduplicated result |
|---|---|---|
| behavior proposed FIND-027-4; I027-04; MNT-027-R2-01 | **REVISED**: functional diagnosis confirmed; correction must include measurement writer/reactivity as well as own-entry reads | FIND-027-4 |
| INTERACTION-027-1: blur during held pointer can complete twice; Enter reaches same path | **CONFIRMED** | Revised FIND-027-2, completion terminal entry |
| INTERACTION-027-2: losing editability strands active preview/listeners | **CONFIRMED** | Revised FIND-027-2, availability terminal entry |
| Follow-up: these are one incomplete producer/preview lifetime, with original Escape path genuinely repaired | **CONFIRMED** | Preserve FIND-027-2; no new ID per event or host |
| Broad closure statements in behavior/invariant/standards/maintainer/system reports | **REVISED in scope** | They establish original R1 scenarios, not held-pointer completion/editability transitions omitted from those reports |
| Inline remoteLoader recreation speculation; direct example cwd observation | **REJECTED as findings** | Compiled dependency-free derived loader and passing canonical workspace consumer check/build do not establish either regression. No ledger entry. |
| Inferred export-return annotations/test-name conventions | **REJECTED as blocking findings** | No behavioral/public-contract consequence; no remediation round justified. |

Two active deduplicated findings follow. No orchestrator/human-originated additions receive FIND IDs; no genuinely new ID beyond FIND-027-4 is needed.

## FIND-027-2 — REVISED / INCORRECT

**Discovery sources:** prior FIND-027-2; INTERACTION-027-1; INTERACTION-027-2; focused followup-review. **Obligation:** REQ-036/AC-015 and scenario 3's once-per-completion and read-only/cancellable editing, with R1 preserved keyboard and pointer behavior.

**Locations:** `packages/ui/src/lib/WyrdView.svelte:91-110,150-161,185-215,297-321`.

**Reachability and consequence.** Pointerdown installs a producer independently of whether it has made a draft. Move makes a draft from the pointerdown layout. Native grip blur invokes commit (`305`); Enter invokes it (`174`). Commit publishes/retains the draft and clears it, but never releases the captured producer. Later move writes another draft from the same origin, then up commits again. A non-echo mount is an explicit supported production path. Domain's fresh Chromium proof at 1200x1000 reports y1 at Tab/blur and y2 at later up for one held drag, matching these exact writes. Enter follows the identical commit path by source. Before first move, commit's early return also leaves an already installed producer alive.

On width <720 or removal of onlayout, editable becomes false and removes the captured controls while the surrounding panel remains. Neither transition calls cancel. Removed target listeners/window Escape and draft remain owned by the renderer; there is no lost-capture cancellation. Pointer release after removal need not reach the old element. The preview stays first priority in layouts, influences mobile ordering/height and returns with its editing outline on widening. Domain's actual 1440→600→1440 gallery sequence confirms row5→preview row7→still row7/editing after release. Handler removal follows the same explicitly supported prop boundary by source, although it was not physically probed.

**Selected smallest correction.** Keep renderer-owned release/stop, commit and cancel. Completion ends any active pointer producer **before** the draft/parsed early return and before publishing; it then keeps existing keyboard blur/Enter semantics. Loss of editability cancels the current preview through the existing cancellation owner **before** controls disappear, retaining prior completed state and emitting nothing. Use the existing renderer pre-update effect facility to observe that availability boundary. Do not clear kept or couple this transition to load abort/reload. Keep pointerup/stop idempotent when cleanup already happened, Escape focus-independent, pointercancel noncommitting and new-document/unmount cleanup intact.

This source correction covers both move and resize and all hosts. A per-move editable guard would leave stale draft/listeners; host callback suppression is too late. Suppressing keyboard blur changes existing behavior unnecessarily. Adding a general gesture framework or lost-capture subsystem is unnecessary for these known terminal entries. Guards belong in this renderer because it owns preview and capture lifetime, not in callback consumers.

**Focused closure proof.** Extend existing component event tests; use existing browser capability for actual native focus/capture:

1. Non-echo host with a prior retained sibling edit: hold grip, move, Tab/blur; separately Enter while held. Each publishes once; further move/up cannot alter preview or callback count. Include completion before first move: no callback, later events inert. Cover shared corner producer with focused grip as completion entry and fresh gestures afterward.
2. Hold move and resize, become narrow, release after controls disappear, widen. No callback, no editing outline, preceding effective/retained layout restored. Repeat with onlayout removed/reinstated through the existing component rerender facility. Include focused/unfocused starts so control-removal blur cannot accidentally publish.
3. Preserve ordinary keyboard Enter/blur, Escape, direct pointer Escape, pointercancel, fresh pointerup once, whole retained output/immutability, no additional loads and narrow explicit-height/reading order.

## FIND-027-4 — REVISED / INCORRECT

**Discovery sources:** behavior proposed FIND-027-4; I027-04; MNT-027-R2-01. **Obligation:** REQ-030's accepted document IDs, REQ-036/AC-015's valid placement and valid whole-document completion preserving untouched panels.

**Locations:** `packages/ui/src/lib/WyrdView.svelte:96,107,112-120,150-155,295`; accepted-input producer `schemas.ts:197-201,213-226`; sink `layout.ts:33-40` and renderer boxStyle/current/ordered.

**Reachability and consequence.** A valid panel ID constructor/toString/__proto__ is selected against empty ordinary kept. The inherited function/prototype wins over panel.layout at `113`. place spreads it to `{auto:false}` without coordinates; rendered grid styles/edit origins become invalid and later auto placement can receive NaN. Editing an ordinary sibling makes done retain Object.prototype; callback overlay mistakes that inherited entry for a completed layout and replaces an untouched valid layout with a function/object. Echo can then invalidate the whole view. No editing opt-in is needed for initial corruption. This is local functional corruption, not global prototype pollution or code execution.

Measurements share this source boundary, including the **writer**, not only fallback reads. Installed `svelte/src/internal/client/proxy.js` get trap falls through to inherited values. Its set trap creates a source only for a name absent from the chain or an own writable descriptor. For inherited names, `heights[id]=height` therefore does not establish a numeric own source; `__proto__` is included. Object.hasOwn read guards alone would still leave measurement writes/reactivity broken for those accepted IDs. A null-prototype deep state record alone also loses Svelte's ordinary-object proxy support in the installed implementation. Neither partial fix is the selected correction.

**Independent read-only proof.** Through mise, imported actual candidate dist schema/layout and evaluated the exact selection/callback mappings. Output: input valid; first placement `{auto:false}`; callback invalid under ViewDocumentSchema. Separately imported the installed Svelte proxy and assigned 104 to constructor, toString, __proto__, normal: first three remained non-own values of function/function/object type; normal became own number. Native computed-property objects `{[id]:104}` produced own numeric values for every spelling. These probes created no files. The renderer's current compiled/native binding is the proxy indexed setter described above.

**Selected smallest complete correction.** Keep the existing record-based renderer state and accepted ID contract. Use the nearby native `Object.hasOwn` convention at retained-layout selection, completed callback overlay and measurement fallback; inherited entries never count as edits or numeric measurements. Keep computed-property spread for retained completions: it safely creates an own __proto__ entry and copies only own entries; guards must remain correct after both empty reset and later spread replacement.

For measured heights, use raw immutable record replacement with a computed ID property through the existing Svelte readonly dimension function binding (null getter plus setter), retaining native offsetHeight observation and teardown. This replaces the unsafe deep-proxy indexed write and makes every actual measurement a reactive own entry. Keep initialization/reset behavior and actual recorded zero heights valid; measurements stay transient and never enter document callbacks. No generic dictionary abstraction, schema restriction, host guard, geometry fallback, new observer, map framework, dependency or public API is required. Native own-entry reads are appropriate here because these records own the renderer's arbitrary-ID overrides; guarding CSS/place would merely hide the invalid producer.

The installed compiler independently accepts that native binding with no warnings and emits bind_element_size. This is a feasibility/source check, not a claim that the fix was implemented or its future regression tests already passed. An initial compiler probe used a named import against its CommonJS artifact and failed to import; rerunning with the artifact's default export succeeded. No application gate failed or was changed.

**Focused closure proof.** In existing grid/edit component tests, use valid constructor, toString and __proto__ IDs alongside ordinary siblings. Check authored initial coordinates without a callback; edit ordinary sibling and validate output while untouched special-ID authored layouts/absence remain unchanged; edit each special ID and verify bounded normal completion/cancellation. Force a positive measured height and then a changed height on an **unplaced** special-ID panel: finite occupied rows and following-panel position must update reactively. This last assertion is required to catch a reads-only repair. Keep non-echo cumulative output, feedback/no reload, input immutability and normal IDs. Use existing facilities, no new harness.

## Focused verification boundary and limits

Root's current verification.md records nine exact green selectors, UI/example check/build and codegen, with unchanged candidate source. Those checks prove ordinary IDs and original R1 cases; they omit these newly validated paths. Domain supplies fresh bounded Chromium Escape/blur/breakpoint probes; original/R1 physical geometry evidence remains historical. This validator ran only the read-only schema/geometry/proxy/compiler probes above, not browser or package builds.

For the selected renderer/test write set, run affected exact existing selectors through mise while adding the described assertions:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Pointer edit cancellation$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Grid placement$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Narrow views$'
```

Then the existing file plus UI check/build and diff check are sufficient local regressions. Supplement with bounded existing-browser focus/capture and breakpoint proof; do not introduce a harness or require broad server journeys, Linux/release builds, codegen or repository aggregates for this unchanged schema/API write set. If a new test name is chosen, its exact anchored selector must be recorded and run too. No test weakening, retries or skips are justified.

Skipped implementation, repeat broad verification and fresh browser execution. Risk: the two validated renderer defects remain at this immutable candidate; Linux/live MCP parity and broader platform proof are outside this bounded validation.
