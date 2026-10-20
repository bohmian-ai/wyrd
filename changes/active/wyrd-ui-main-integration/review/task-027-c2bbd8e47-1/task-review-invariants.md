# TASK-027 invariant review — FAIL

## Immutable subject and coverage

Base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; candidate and observed HEAD `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`. Reviewed the cumulative task-owned range, latest remediation range, revision-12 approved spec (REQ-030/036, AC-015 and applicable invariants), original `TASK-027-view-layout.md`, prior verdict/validated findings and `TASK-027-R1-layout-state-and-geometry.md`. No peer report from this attempt was read. No source was changed.

Authorities consulted: supplied AGENTS.md; architecture agent rules; reference router; spec-driven development, maintainer style, TypeScript and testing-workflow references; Wyrd design client boundary and doctrine; repo UI skill, Svelte/testing references and brand DESIGN.md. No `.codegraph/` directory exists, so navigation used repository searches and git diff. This is task acceptance and state-flow review, not an independent whole-repository standards certification.

Expanded the starting map through complete `WyrdView.svelte`, `layout.ts`, view-document schemas and mount/remote-load owner; renderer loading and action siblings; Chart/CardHeader sizing; catalog schema projection; all production `onlayout` sites (gallery, example, AppBridge); and all five TASK-027 focused test bodies. TASK-003 remediation in the shared renderer is a separate authorized owner and is not attributed as TASK-027 drift. Unrelated merged changes in the cumulative range are excluded by the subject's scope attribution.

## Producer, state and consumer audit

| Authority / state | Producers and transitions | Consumers and conclusion |
|---|---|---|
| Authored document | `schemas.ts:180-227` validates bounded optional Layout and unique nonempty panel IDs; parsed input changes reset kept/draft in `WyrdView:102-109` | `layouts`, content-only load identity, callback construction and schema/catalog. ID strings remain unrestricted and expose I027-04 below. |
| Completed authored edits | Keyboard Enter, grip blur, pointer-up converge on `commit:150-156`; `kept` replaced as raw state | `layouts:112-114` and callback both now include every completed edit. Untouched panels preserve authored collision positions and missing layout. Ordinary-object inherited entries are the remaining common failure source. |
| Preview and pointer lifecycle | Key arrows and pointer movement produce draft; Escape reaches cancel; cancel removes listeners/capture through release; pointercancel discards; new input and destroy release gesture | All move/resize paths reuse edit/current and renderer state. FIND-027-2 source diagnosis is closed: after cancel, subsequent pointer events have no registered producer or completion listener. |
| Physical occupancy | `offsetHeight` binding at `WyrdView:295` feeds transient heights; `rowsFor` and `place` reserve whole rows | Fixed 40px desktop tracks, rowsHeight, collision placement and current edit-start share the same row extent; align-self:start preserves intrinsic box height. No measured value is serialized for untouched panels. |
| Narrow layout | Measured view width selects `<720`; ordered derives resolved `(y,x)` | Full-width styles, auto mobile tracks, explicit h-derived heights and omitted edit controls. Source and supplied tests support ordinary narrow rendering. |
| Reads and component state | content signature omits only layout; loader derives from stable source identity; old controllers aborted | Local edits and host feedback retain existing panels/reads. Actual content/range/executor changes still reload. Chart retains its own box measurement; components gain no layout prop. |
| Host lifetime | gallery/example replace visit-local doc; mountView forwards onlayout; AppBridge destroys old instance on next tool result and uses a non-echoing handler | Common renderer keeps current render edits, with no persistence or host-side merge machinery. All hosts share the dictionary defect; none owns an appropriate correction. |

Existing-owner comparison: pure geometry in `layout.ts` serves the existing WyrdView placement owner; no previous reusable grid owner or installed layout workflow was displaced. Keeping measurement and gesture lifetime in WyrdView is appropriate. `view.ts` already owns mounting, while component Chart owns plot dimensions; no second renderer, loader, persistence layer, component size input, grid dependency or per-host layout authority was introduced. I027-04 requires ordinary own-entry dictionary semantics in this owner, not a new abstraction.

## Full acceptance matrix

“Supplied” verification refers to this review's `verification.md` and R1's historical browser measurements; it is not a claim that this reviewer reran those builds or browsers.

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Optional whole-unit Layout, required fields and all 12-column / h bounds; reject width | `schemas.ts:180-201`; strict PanelSchema and refinement | Supplied exact Layout document contract selector and codegen check | PASS |
| Valid document IDs and layouts render at their authored/resolved grid coordinates | `WyrdView:96,107,113`; ordinary kept lookup can replace a valid authored Layout with Object constructor | Read-only candidate-source probe below; normal Grid placement green does not cover prototype names | FAIL |
| Mixed and all-unplaced panels span full width below earlier panels at content height | `layout.ts:22-42`; WyrdView measurement, align-self:start and fixed desktop tracks | Grid placement selector; supplied short-header/tall-chart, load-settling and width-change browser observations | PASS |
| Collision push-down is document-order, nonoverlapping, rendering only | pure `place` clones explicit layout and advances y to prior bottom; only kept edits serialized | Grid placement and Layout editing assertions, supplied geometry browser evidence | PASS |
| Components fill resized boxes; overflow remains internal; chart redraw | WyrdView `.panel/.fill` fixed box and overflow; Chart box width/height bindings | Historical original visual acceptance and R1 geometry evidence; supplied build/check | PASS |
| No handler gives read-only view; handler enables move/resize | optional prop and mount option; editable/handle markup | Layout editing selector, source for all host sites | PASS |
| Pointer and keyboard snap and respect bounds, with keyboard move and Shift resize | `current`, shared pure `edit`; drag row/column pitch | Layout editing and Pointer edit cancellation selectors; supplied browser drag evidence | PASS |
| Escape cancels preview and stops pointer producer; cancellation cannot later complete | `cancel:159-162`, drag stop/remove/capture cleanup at `185-215` | Pointer edit cancellation selector; R1 focused/unfocused move and corner browser observations | PASS |
| Completion emits one whole updated document, preserving previous sibling edits and untouched authored fields | `commit:150-156` overlays all kept entries, but inherited `done[panel.id]` is mistaken for a completed edit | Normal two-panel non-echoing test passes; valid constructor-ID callback fails schema in candidate-source probe | FAIL |
| Rendering/editing do not mutate supplied document or persist measurements/collision placement | cloned placement and updated document; raw state retains authored drafts | Grid placement/Layout editing input equality and untouched c/d assertions | PASS |
| Layout-only updates and same executor do not reload panels | content signature `72-75`, stable source/loader `61-70` | Layout editing feedback test and supplied dependent-load regression selector | PASS |
| Narrow view stacks by `(y,x)`, retains explicit h and disables controls | narrow/ordered/boxStyle; narrow CSS | Narrow views selector; supplied 375px browser observations | PASS |
| Gallery, example and AppBridge demonstrate layout; edits confined to render/visit | gallery state/onlayout, example state/onlayout, AppBridge destroy/remount | Supplied example/workspace check/build and recorded human acceptance 2026-10-20; original desktop/mobile light/dark browser evidence | PASS |
| No new dependency/framework, persistence, size input, width alias, add/remove/duplicate/collapse, shipped-page placement | task-owned diff restricted to layout extension and direct consumers; component catalog entries unchanged | Source/diff and document-contract catalog assertions | PASS |
| R1 behavioral diagnosis and focused proof, without hiding failures | smallest shared-owner corrections; focused tests added, no skip/allow/timeout weakening | Supplied RED/GREEN record, exact selectors, fresh checks. Geometry RED came from prior source-backed review rather than manufactured repeat churn | PASS |

## Prior finding closure

| Prior ID | Candidate source and proof | Assessment |
|---|---|---|
| FIND-027-1 | `commit` now computes done from all kept plus current draft and uses it for every panel; non-echoing a/b test verifies both edits, untouched unplaced/colliding siblings, feedback/no reload and input immutability | Original lost-sibling-edit cause closed. New inherited-entry defect below also affects serialization, but its producer and necessary correction are distinct; this is not automatic closure based on green checks. |
| FIND-027-2 | release owns move/up/cancel/window-Escape listeners and capture; Escape calls cancel regardless of focus; document transition/unmount cleanup exists | Closed. Focused/unfocused move and corner cancellation followed by movement/release and fresh successful gesture are covered in recorded component/browser evidence. |
| FIND-027-3 | auto occupancy now derives from measured panel height through rowsFor; fixed CSS rows; intrinsic align-self:start; edit-start reads placements rather than independently rounding height | Closed for the diagnosed stretched-track source. Supplied physical measurements distinguish short header y4=208px from tall chart push-down, settling and width changes. DOM style-only tests alone would not have sufficed. |

## Proposed finding I027-04 — INCORRECT: inherited dictionary entries corrupt valid panel layouts and emitted documents

**Violated obligations:** REQ-030's validated view-document contract, REQ-036 valid layout rendering and whole-document completion, AC-015. PanelSchema intentionally accepts any unique nonempty string ID; reserving JavaScript prototype names is not approved behavior.

**Exact locations:** `packages/ui/src/lib/WyrdView.svelte:96` and `107` initialize/reset kept as `{}`; `113` uses `kept[panel.id] ?? panel.layout`; `155` uses `done[panel.id]` as the completed-layout test. Sibling dictionary at `116/120/295` holds measurements by the same unrestricted ID and must be included in the own-entry audit. Schema producer is `schemas.ts:199-201,219-225`.

**Reachable producer-to-sink trace:** Give the supported WyrdView/mountView/AppBridge surface a valid Chart panel ID `constructor` with authored `{x:6,y:0,w:6,h:4}`. No edit is necessary: initial kept is `{}`, so `kept.constructor` is the inherited `Object` function. The fallback ignores the valid authored layout and passes the function to place; spreading it produces `{auto:false}` with no x/y/w/h. boxStyle then derives NaN/undefined coordinates and current/edit cannot recover a valid layout. With another ordinary panel `a`, complete an edit of a. The new done object retains Object.prototype, so commit additionally treats untouched constructor as an edited sibling and sets its layout to the inherited function. The advertised whole-document result now fails ViewDocumentSchema; a feedback host can replace a valid view with an invalid document. `toString`, `hasOwnProperty`, and `__proto__` are other supported IDs with the same source. This is functional corruption, not a claim of global prototype pollution or remote code execution.

**Independent read-only reproduction:** Ran through mise, importing candidate `schemas.ts` and `layout.ts` with Node type stripping. Constructed the valid two-panel document above, called ViewDocumentSchema.parse, then evaluated the exact layouts and commit mappings. Output:

```text
initial placements [{"x":0,"y":0,"w":6,"h":4,"auto":false},{"auto":false}]
input valid true callback valid false sibling layout type function
```

The command exited 0; no file or test harness was created. The installed Svelte proxy's get trap falls back to Reflect.get for inherited properties; deep state measurement does not provide own-key semantics. Raw kept state is an ordinary object directly.

**Writers, callers and siblings enumerated:** initialization and parsed-document reset create kept; commit replaces it; keyboard Enter, blur and pointer-up are the only completion callers; layouts and callback mapping are both kept consumers. DOM binding writes heights and placement reads it. All renderer host sites use the same owner. Placement already uses Map for ID indexing and has no inherited-key lookup, while no existing host merge or alternative layout state owner should be extended to compensate.

**Smallest testable correction:** Keep the renderer and schema unchanged; give ID-indexed retained-layout and measurement lookups own-entry semantics, including after reset and immutable spread-based completion. In particular, an inherited entry must never override authored layout or count as an edited panel. Use native own-property checks or an equally small prototype-safe representation; adding reserved-ID validation, new infrastructure, host guards or component changes would unnecessarily alter the contract. Preserve accumulation of actual authored edits, measurement reactivity, untouched panel representation, load identity and document immutability.

**Focused closure:** In existing Layout editing/Grid placement tests, use a valid panel ID `constructor` (and representative `__proto__`/`toString` IDs): verify authored grid position initially; complete an ordinary sibling edit and assert untouched special-ID layouts remain intact and callback validates; edit the special-ID panel itself and assert valid bounded result. Exercise an unplaced special-ID panel through measurement, retaining content-height semantics. No new test harness or full server journey is needed to force this pure renderer defect.

## Verification and result

Supplied nine exact selectors, workspace and example check/build, codegen and diff checks are green at the immutable candidate. They establish normal layout paths and R1 regression closure, but omit unrestricted-ID dictionary semantics. This reviewer independently ran the candidate-source schema/placement/callback probe above and verified HEAD. No material DRIFT, missing required new abstraction, invalid task instruction or spec decision was identified.

**Overall result: FAIL**, with one proposed material finding I027-04 for independent validation. FIND-027-1..3's original diagnoses are closed with the evidence above.

Skipped rerunning package builds and browser sessions already supplied by the orchestrator. Real-browser geometry proof is historical; Linux execution is not evidenced, and the new ID-path probe is source-level rather than a DOM test.
