# TASK-027 independent behavior review

Result: **FAIL**.

Subject: repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`, candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`. Approved spec revision 12, `changes/active/wyrd-ui-main-integration/spec.md`; original task `tasks/TASK-027-view-layout.md`. Source remained immutable. This is an independent discovery report; no peer reports were read.

## Authority, scope and navigation

Applied AGENTS.md and architecture/agent-rules.md, the reference router, spec-driven-development, maintainer-style, TypeScript and testing guidance, the UI skill and brand DESIGN, and the approved REQ-030/036 and AC-015 contracts. The shared task subject identifies the complete cumulative range. The range contains planning/evidence updates and merged upstream changes; the TASK-027 implementation is attributable to `d33de26e4` and `d90d98a12`. TASK-003's existing rendering/loading owners were inspected as dependencies rather than re-reviewed as new TASK-027 work.

Read the complete changed UI owning modules and their relevant consumers: `schemas.ts`, `layout.ts`, `WyrdView.svelte`, `view.ts`, app `main.ts`, generated catalog layout entries, UI library scenarios, gallery, example panel definitions and `/view` route. Existing alternative is the TASK-003 WyrdView renderer and mountView bridge; the change correctly extends those rather than introducing another rendering or loading framework. The new geometry module earns its place as pure shared placement/edit logic. The findings below concern behavior within these existing owners, not demands for replacement architecture.

## Acceptance matrix

All UI paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui` unless qualified.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Optional whole-unit layout with x/y/w/h bounds and x+w≤12 | `packages/ui/src/lib/schemas.ts::LayoutSchema`, strict PanelSchema | Exact layout document contract scenario passed; rejects partial, fractional and out-of-range layouts | PASS |
| Reject width and expose layout only in view schema | Strict PanelSchema; catalog view schema and unchanged component input contracts | Contract scenario asserts width rejection and no component layout/width/height keys | PASS |
| Twelve columns, 40px rows and placed panel positions | `layout.ts` constants and placement; WyrdView boxStyle and `.panels` | Placement scenario checks inline strings, not browser track sizes; B027-03 identifies auto-track displacement | FAIL |
| Unplaced panels are full width at content height below earlier panels; mixed documents preserve the grid | `place` assigns every auto panel h=1; `.panels` permits intrinsic track growth | Pure/in-DOM style assertions pass but cannot prove physical mixed geometry; B027-03 | FAIL |
| Collision push-down does not mutate input or overlap panels | Pure `place` repeatedly advances to the bottom of an overlapping earlier rectangle; new placement objects | Grid placement scenario and historical browser collision evidence; no input writes | PASS for fixed boxes; mixed geometry covered by B027-03 |
| Components fill boxes, overflow remains in panel, charts redraw | `.fill` grid with min-height:0 and overflow:auto; fixed outer panel height | Historical browser evidence for chart shrink/grow and overflow; no fresh browser replay | PASS within recorded evidence |
| No handler means no affordances; handler exposed through mountView | `editable`, conditional handles; MountViewOptions forwarded unchanged | Layout editing scenario proves absent controls without handler | PASS |
| Arrow move, Shift+arrow resize, bounds and keyboard Escape | `key` and shared pure `edit` | Layout editing scenario exercises keyboard move/resize, cancel and bounds | PASS for keyboard-only sequence |
| Pointer move/resize grid snapping and cancellation | `drag` uses same edit and cell pitch, but pointer listener outlives Escape | Historical successful pointer moves/resizes; no cancel proof; B027-02 | FAIL |
| One whole updated document per completed edit; host chooses what to keep | `commit` writes kept for display, but constructs callback from parsed original panels and current panel only | Existing test repeatedly edits only panel a; B027-01 demonstrates cross-panel omission | FAIL |
| Editing and host echo do not reload panels | Content key strips panel layout; loader derives from stable load/executor identity | Exact layout editing test and TASK-003 dependency regression scenarios passed in orchestrator rerun | PASS |
| Input document never mutated, rendering never writes collision results back | Zod parse output, copied placements and panel replacement | Original doc equality assertions and source trace | PASS |
| Below 720px stack in (y,x), preserve h, disable editing | Measured container width, ordered panels, boxStyle height, editable | Exact narrow scenario passed, historical 375px browser evidence | PASS within recorded evidence |
| Gallery/example/AppBridge wiring; render-only MCP edits | Gallery and example handlers replace local doc; app mount passes no-op; new render destroys old view | Historical visual approval and task check/build evidence; no persistence writes | PASS, with handler-output defect in B027-01 |
| No add/remove/duplicate panels, collapsible rows, persistence, new dependency or component size input | Changed symbols and manifests; no such workflow added | Diff/source inspection | PASS |
| Theme geometry and shipped-page ownership preserved | Existing tokens and container-owned arrangement, unchanged shipped layouts | Human approval and light/dark browser evidence; no fresh visual replay | PASS within recorded evidence |

## Proposed findings

### B027-01 — INCORRECT: completed edits to other panels disappear from the whole-document callback

Obligation: REQ-036 and scenario 3 require each completion to report the whole updated document; hosts decide what to keep. Location: `packages/ui/src/lib/WyrdView.svelte:134-139`, particularly line 139.

Reachable producer-to-consumer path: mountView exposes an onlayout callback without any update API, and the MCP artifact itself opts in using a no-op handler (`packages/ui/src/app/main.ts:26`). WyrdView therefore intentionally retains edits in `kept` until a new host doc arrives. Edit panel a and commit; kept[a] holds its new layout. Edit panel b and commit without replacing the input document. Rendering uses both kept[a] and kept[b] at lines 108–111. The second callback, however, maps `parsed.data.panels` and replaces only b. Panel a's callback layout is its original value, contradicting what the reader is seeing. A mountView host receiving or later consuming the second document loses a's completed edit.

Sibling consumers/writers: keyboard Enter, grip blur and pointer-up all call this one commit owner. Gallery and example immediately echo callbacks and usually conceal the omission; mountView and the no-op MCP handler do not. `kept` is the only retained completed-edit source; `place` is a rendering result and must not be serialized as if authored.

Smallest safe correction: construct the callback using all completed authored layouts in kept after installing the current change. Preserve original untouched panel fields, avoid serializing collision-induced placements, retain immutable input and existing no-reload content key. No persistence or new API is needed.

Focused closure proof: mount a two-panel view with a recording handler that does not echo doc. Commit a change to a, then a change to b. The second callback must contain both completed changes and equal the authored layouts shown; original doc and load counts remain unchanged. Cover both mountView and WyrdView through the same owner rather than duplicate fixes.

### B027-02 — INCORRECT: Escape does not cancel the active pointer interaction

Obligation: REQ-036 says Escape cancels; scenario 3 requires pointer editing to be cancellable with the same results. Locations: `WyrdView.svelte:153-154` and `159-178`.

Reachable path: focus a move handle, begin dragging that same handle and dispatch pointer movement. Press Escape while holding the pointer. The handle's key handler clears draft, visibly restoring the pre-edit layout, but the pointermove/up/cancel listeners installed by drag remain live and retain the original `from`. A subsequent pointer movement, ordinary while releasing a drag, recreates draft at line 168; pointer-up then commits it at line 173. An edit explicitly cancelled by the reader is emitted after all. For a pointer drag begun while the handle is not focused, there is additionally no pointer-session Escape listener on its event path, so cancellation is not guaranteed at all.

Sibling paths: the corner resize shares the same drag owner; keyboard-only cancellation works because it has no surviving pointer producer. The handler's current cancellation test exercises only that keyboard-only case. Pointercancel clears the draft but does not make Escape terminal.

Smallest safe correction: end/cancel the active pointer interaction in the existing WyrdView interaction owner when Escape is received, remove its listeners/release capture as appropriate, and prevent further movement/up from recreating or committing its draft. Apply to both move and corner resize without changing normal blur/Enter commit semantics or adding a framework.

Focused closure proof: for both move and resize, start drag, move, Escape, move again, release. Assert original effective layout is restored, no onlayout call occurs and no stale dragging listeners affect a later independent edit. Include the ordinary unfocused pointer start in browser evidence or the existing event-level fixture.

### B027-03 — INCORRECT: an auto-height panel stretches the nominal 40px grid used by all later placements

Obligation: REQ-036 fixes rows at 40 CSS px while allowing content-height unplaced panels and mixed documents; scenario 2 and AC-015 require those documents to render at their grid positions. Locations: `packages/ui/src/lib/layout.ts:28-33`, `WyrdView.svelte:117-130`, `.panels` at lines 310–316.

The placement producer models every unplaced panel as exactly one row (`h:1`), independent of its rendered content. Rendering then leaves that panel's height intrinsic but uses `grid-auto-rows:minmax(40px,auto)`. A content-height Chart or CardHeader taller than 40px enlarges that single CSS track; all subsequent CSS grid lines shift, while collision resolution and drag snapping still use ROW+GAP=52px. For a concrete accepted document, put an unplaced chart first, then a fixed panel at y=4. If the chart has 200px intrinsic height, grid line 5 lands at 368px instead of 208px. The fixed panel has not collided with the logical one-row auto rectangle, yet its declared y is rendered on a stretched grid. Editing or converting the auto panel to a fixed layout changes the coordinate mapping under later panels.

This is a shared geometry-source mismatch, not a request to change the approved content-height behavior. The existing tests explicitly place an unplaced chart before a later fixed chart, but assert only grid-row strings; jsdom does not resolve track sizes. Recorded gallery examples put their unplaced panel last, so their visual success does not disprove this path.

Sibling consumers: place uses these rectangles for every collision and unplaced bottom calculation; WyrdView boxStyle/current and pointer drag assume the same fixed pitch; the narrow ordering uses placements. All custom hosts can supply mixed documents. No other measured geometry owner exists to reuse: current() measures auto height only when beginning an edit, too late for normal placement.

Smallest safe correction boundary: make the existing placement/render owner account for unplaced panels' actual content extent on the fixed 40px grid, so auto content remains at content height while its occupied grid extent is represented consistently for following placements and collisions. Keep the pure collision step and existing view owner; do not mutate the source doc, freeze auto content at 40px, or add component size inputs. Resolve auto geometry once at that owner rather than compensate separately in pointer, collision and sorting consumers.

Focused closure proof: browser geometry for an unplaced tall panel followed by a fixed panel at a noncolliding y and by a colliding one. Assert physical row pitch, content-height retention, push-down/no overlap, and unchanged doc, then edit the auto panel and confirm no displacement inconsistent with the same pitch. Retain the no-layout and mobile cases. A string-only grid-row assertion cannot close this finding.

## Verification and limits

The orchestrator's independent verification.md records all four exact TASK-027 commands passed at unchanged candidate, one selected scenario each; TASK-003 scenarios 1/3, all seven ui-library tests and checks also passed. Original task records 37 tests, UI/example checks/builds, codegen and browser/human acceptance. I inspected the assertions and their coverage rather than treating those green results as universal proof. No new source, test, dependency, or harness was written. A read-only Svelte compiler invocation confirmed the example's inline remoteLoader is compiled to a stable derived value; I rejected the hypothesis that the example necessarily recreates its loader after each local layout update.

No browser tests were rerun by this reviewer. B027-01 and B027-02 are directly traceable event/state paths; B027-03 is a source-backed CSS geometry consequence requiring browser closure. Linux bundles, TASK-003 live degraded terminals and production MCP registration remain outside this task. Optional style preferences and pre-existing foundation issues are excluded.
