# TASK-027 independent invariant review

Result: **FAIL**.

Subject: repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`, candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`. Task implementation is `d33de26e4` plus `d90d98a12`; reviewed cumulative UI diff and current owners, including TASK-003 dependencies. No reviewed source was edited. This report is independent of peer reports.

Authority: approved spec revision 12, REQ-036 and AC-015, revised REQ-030 document contract; original `tasks/TASK-027-view-layout.md`; repository AGENTS, agent rules, spec-driven-development and maintainer-style references, testing workflows and UI brand design. Current user approval supplies human visual acceptance, not an exception to handler or cancellation behavior.

## Navigation, ownership and reuse

- `packages/ui/src/lib/schemas.ts::LayoutSchema/PanelSchema` produces the validated document; `view.ts::validateView`, WyrdView's `parsed`, and `scripts/catalog.ts` consume that owner. Bounds stay in the existing schema, not a second handwritten validator.
- `layout.ts::place` copies layouts, places unplaced panels after earlier occupied rows, and resolves collisions by monotonically increasing the later panel's y. `edit` clamps changes. Both are pure geometry owned by each invocation; no competing grid service or added dependency exists.
- `WyrdView.svelte` owns document-derived placements, runtime `kept`, one in-progress `draft`, render order, component containment, edit events and content/load identity. Existing Chart owns redraw when its box changes; layout does not add component size fields.
- Complete edit caller set: grip keyboard → `key` → `edit` → `draft`; grip/corner pointer → `drag` → `edit` → `draft`; Enter, blur and pointer-up → `commit`; Escape and pointer-cancel discard draft. `layouts` consumes `draft`, then `kept`, then authored layout. The emitted document is a separate consumer of this same state.
- Host set: gallery and example `/view` feed the callback document back into the renderer; MCP App and valid `mountView` consumers can leave original props in place. MCP App deliberately passes a no-op callback. The task expressly requires render-local edits, so the latter path is production behavior, not hypothetical API misuse.
- Loading stays with the existing renderer/loader owner. Content identity strips only `layout`; `load ?? executor` identity controls loader replacement. These extend existing workflows rather than duplicate them.

## Obligation matrix

Locations below are under `crates/wyrd/wyrd-server/wyrd-ui` unless otherwise indicated.

| Requirement, criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Whole-unit optional layout, x/y/w/h bounds and x+w constraint | `schemas.ts:183-196`, strict `PanelSchema` | Exact layout-document-contract scenario; recorded codegen check | PASS |
| Reject `width` and partial/fractional/out-of-range layout with field detail | Strict panel/layout schemas; existing `inputProblem` mapping | Contract scenario covers width, missing h, fractional h, bounds and sum | PASS |
| Publish layout in view schema; catalog component inputs stay size-agnostic | Generated `catalog.json` view layout addition; unchanged component entries | Contract scenario checks view schema and no layout/width/height component keys | PASS |
| 12 columns, 40px rows, matching geometry constants | `layout.ts` COLUMNS/ROW/GAP, `rowsHeight`; WyrdView `.panels`, `boxStyle` | Grid-placement scenario plus recorded real-browser evidence | PASS |
| Unplaced panels full-width, content-height, below earlier panels | `place` computes bottom; auto placements omit fixed CSS height | Grid-placement scenario covers mixed and all-unplaced documents; browser evidence supplied | PASS |
| Collisions move later panels down, no document rewrite | `place` copies each layout, repeated overlap search increases y to hit bottom | Grid-placement scenario checks collision placement and original input equality | PASS |
| Components fill fixed boxes and overflow internally; charts redraw | WyrdView `.fill` min-size/overflow/grid; Chart's existing box measurement | Human-approved browser resize/redraw/overflow evidence; no fresh browser run | PASS |
| Editing absent without callback | `editable` gates grip and corner | Layout-editing scenario checks absent handler | PASS |
| Keyboard move/resize constrained to whole grid units | `key` and pure `edit` | Layout-editing scenario checks move, resize and clamping | PASS |
| Pointer move/resize snap with same geometry | `drag` computes rounded dx/dy and delegates to `edit` | Recorded gallery/AppBridge pointer evidence | PASS |
| Escape cancels editing, including pointer edit lifecycle | `key` clears draft but `drag` callbacks remain live | Existing scenario tests only keyboard Escape; pointer-cancel is a different event | FAIL — I-027-02 |
| One callback per completed change, carrying whole updated document | Enter/blur/up funnel into `commit`, which clears draft, but callback rebuilds from original parsed panels | Existing editing scenario edits only one panel before host feedback; source-body check demonstrates lost sibling edit | FAIL — I-027-01 |
| Rendering/editing never mutate input | Schema produces validated envelope; placement and edits copy; commit builds new envelope/panels | Original-doc equality assertions in placement/editing scenarios; source trace | PASS |
| Layout-only edits and host feedback never reload panels | Layout-stripped `content`; loader derives from `load ?? executor` identity | Exact layout-editing regression and TASK-003 scenarios; installed Svelte compiler check described below | PASS |
| Below 720px, full-width (y,x) stacking, retain h, editing off | `narrow`, `ordered`, `boxStyle`, `editable` | Exact narrow-view scenario; recorded 375px evidence | PASS |
| Gallery/example/AppBridge demonstrate layout; MCP edits last one render | Gallery/example callback wiring; `src/app/main.ts:23-26` remounts on tool result with no-op handler | Recorded gallery/AppBridge evidence and human acceptance; historical example check/build | PASS for local display lifecycle; handler completeness separately fails |
| No persistence, saved views, per-viewer memory, add/remove/duplicate, collapse, dependency or component size inputs | Entire task UI diff adds only local renderer state, schema/geometry, host opt-in and examples | Static task diff and contract test | PASS |

## Proposed findings

### I-027-01 — INCORRECT: whole-document emission omits earlier retained panel edits

**Obligation:** REQ-036 editing and AC-015 require each completed change to report the whole updated document. Host feedback is optional; MCP App explicitly retains edits only inside the current render.

**Location:** `packages/ui/src/lib/WyrdView.svelte:134-139` (`commit`), especially line 139; related state consumers at lines 95-110. Exposed through `view.ts:34` and `mountView`, and reached by the MCP App's no-op host at `src/app/main.ts:26`.

**Producer → consumer:** commit A writes `kept[A]`. Rendering correctly reads that value. Commit B writes `kept[B]`, so the visible view contains both edits. Its callback instead maps `parsed.data.panels`, replacing only B; A comes from the original document. This same shared producer serves keyboard Enter, blur and pointer-up, so fixing individual event handlers would duplicate the correction.

**Concrete path:** mount two panels with a callback that records documents but does not immediately re-pass props. Move A from y=0 to y=1 and complete. Move B from y=0 to y=2 and complete. Display state retains `{A.y:1,B.y:2}`, while the second callback contains `{A.y:0,B.y:2}`. A host later applying the latest whole document loses A's accepted change. That host need not add persistence; recording or applying updates later is already within this callback contract.

**Proof:** ran an in-memory assert check with the unchanged `commit` body extracted from candidate source and its actual `parsed/draft/kept/onlayout` dependencies. Two consecutive calls produced:

```json
{"kept":{"a":{"x":0,"y":1,"w":6,"h":4},"b":{"x":6,"y":2,"w":6,"h":4}},"secondCallback":{"panels":[{"id":"a","layout":{"x":0,"y":0,"w":6,"h":4}},{"id":"b","layout":{"x":6,"y":2,"w":6,"h":4}}]}}
```

This is a source-body check, not a new DOM test. The existing DOM scenario never completes edits to two different panels before feedback, so its green result does not contradict it.

**Smallest safe correction:** correct `commit` once to emit every retained completed layout from its existing `kept` owner while preserving unchanged panels and all other document fields. Do not emit resolved collision push-down as authored changes, add persistence, require host feedback or reload panels. Add a focused renderer assertion that commits A then B without rerendering props, verifies the second callback includes both accepted layouts and original input is unchanged, then re-passes that second document without losing either edit or reloading.

### I-027-02 — INCORRECT: Escape clears a pointer preview but does not cancel its transaction

**Obligation:** REQ-036 and TASK-027 scenario 3 require editing to be cancellable with Escape; the task makes this common to pointer/keyboard editing.

**Location:** `packages/ui/src/lib/WyrdView.svelte:153-154` (`key` Escape) and `:159-178` (`drag` listeners, especially pointermove and pointerup).

**Producer → consumer:** a focused grip receives pointerdown and installs native move/up/cancel listeners. A move publishes `draft`; Escape clears `draft` but leaves those listeners active. Another move republishes the same cancelled drag from its original `from`, and pointer-up calls `commit`, emitting an accepted edit after cancellation. No new pointerdown is required. Pointercancel does terminate the listeners, but pressing Escape is not routed to that end path. The resize corner uses the same producer and lifecycle.

**Concrete path:** focus the move grip (a normal keyboard-accessible state), pointerdown on it, pointermove at least one grid row, press Escape, continue moving the held pointer, release. The first preview disappears at Escape, then returns and is accepted. This is reachable even without relying on focus behavior: focus already belongs to the grip, and `preventDefault` on pointerdown preserves it.

**Existing proof gap:** the editing scenario checks ArrowDown → Escape without an active drag. Recorded pointer evidence covers successful moves/resizes, not Escape during a held pointer gesture. Neither exercises listener reactivation after Escape.

**Smallest safe correction:** make Escape end the current pointer edit through the existing renderer's gesture lifecycle, including listener/capture cleanup, before restoring the pre-edit layout. Preserve keyboard Escape, pointercancel, a single callback on ordinary completion, snapped geometry and local-only edits. Do not add a drag framework or separate pointer layout state owner. Focused proof: begin a held pointer move, change preview, Escape, send a further move and pointer-up, assert original layout and zero callbacks; begin a fresh gesture and verify it completes once. Apply the same lifecycle to resize since it shares `drag`.

## Additional falsified concern

I checked whether the example's inline `remoteLoader('/view/load')` creates a new identity on each document edit. The installed Svelte compiler emits a cached `$.derived(() => remoteLoader('/view/load'))` and the child reads that derived source; changing `doc` alone does not invalidate it. This is not a finding and needs no hoisted loader or extra guard.

## Verification and limits

I consumed the immutable task evidence and the orchestrator's exact-command reruns in `verification.md`: all four focused TASK-027 scenarios, TASK-003 regression scenarios, UI suite and check are green. I independently traced every edit writer and callback consumer and executed the unchanged commit-body check above. No peer conclusions were read. No source changes, extra test files, browser sessions, broad aggregates or platform builds were performed.

Historical human/browser evidence remains useful for visuals and successful gestures; it does not exercise the two retained-edit/cancellation gaps. Linux artifacts are a release-workflow matter recorded in TASK-003, not a new TASK-027 demand. This review does not propose persistence or other deferred capability work.
