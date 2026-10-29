# TASK-027 interaction domain review

Result: **FAIL**.

## Immutable subject and scope

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`. Reviewed the cumulative range and task implementation commit `d33de26e4`, distinguishing later unrelated upstream changes from TASK-027. No source modified; this report is the only write.

Boundary: document layout to rendered grid; keyboard and pointer editing through draft, preview, completion, cancellation and host callback; responsive disabling; public `mountView` and MCP App lifecycle. This is an independent interaction review, without peer-report inputs.

Authority: approved spec revision 12 REQ-036 / AC-015 and original `tasks/TASK-027-view-layout.md`; repository AGENTS.md and agent rules; spec-driven development and maintainer-style references; repo UI skill, `brand/DESIGN.md`, and UI developer-UX/testing references. Layout remains client presentation and per-render state, not durable server behavior. No new security, persistence or product decision is needed for either finding.

## Boundary and source coverage

| Boundary | Source and evidence | Assessment |
|---|---|---|
| Valid whole-unit box and bounds | `packages/ui/src/lib/schemas.ts::LayoutSchema`; `layout.ts::edit` | Schema rejects invalid positions/sizes; move and resize clamp horizontal bounds, nonnegative y and h 1–48. |
| Placement and containment | Full `layout.ts::place`, `rowsHeight`; `WyrdView.svelte::layouts`, `placements`, `boxStyle`, `.panel`, `.fill`, `.panels` | Existing renderer extended, pure geometry extracted without a competing layout framework. 12 columns / 40px rows / 12px gaps agree with CSS. Collision displacement is rendering-only. Overflow scrolls in `.fill`. |
| Keyboard and focus | `WyrdView.svelte:125-155, 258-270` | Semantic labeled button, visible focus, arrows and Shift+arrows use shared `edit`; Enter commits, blur commits pending draft, keyboard Escape clears draft without reporting. |
| Pointer move/resize | `WyrdView.svelte:158-179, 269, 287-289` | Grip and corner share snapped geometry; pointerup commits, pointercancel clears. Escape does not terminate an active pointer edit (I-02). |
| Completion / callback | `WyrdView.svelte:95-110, 133-139`; `view.ts::MountViewOptions`, `mountView` | Handler propagated without persistence. Callback document omits previously retained edits to other panels (I-01). |
| Mobile and read-only | `WyrdView.svelte::width`, `narrow`, `editable`, `ordered`, `boxStyle`; NARROW=720 | No handles without handler or below 720; mobile uses full width and `(y,x)` order, retains fixed h. Fixed-height mobile scroll is intentional approved behavior. |
| Host lifecycle | `src/app/main.ts::app.ontoolresult`; gallery `+page.svelte`; `examples/ui-app/src/routes/view/+page.svelte` | Gallery/example reapply callback document; MCP uses no-op callback and keeps renderer state until next tool result destroys/remounts. Non-round-tripping host is explicitly supported and makes I-01 reachable. |
| Load isolation | `WyrdView.svelte::source`, `loader`, `content`, loading effects | Layout omitted from load content key; editing state does not itself change loader identity or data content. No finding on the content-key implementation. |
| Proof | Four TASK-027 test bodies in `src/lib/ui-library.test.ts`; task implementation evidence; orchestrator `verification.md` | Recorded focused checks pass. Existing edit test repeatedly edits only panel a; no cumulative cross-panel callback assertion or Escape-during-drag assertion. |

Compared new machinery against prior renderer and callers: `WyrdView` already owns arrangement, loads and actions; it is the appropriate owner of draft/retained presentation state. `layout.ts` is pure geometry and not a duplicate registry, loader, store or transport. `mountView` projects existing component behavior rather than introducing another editor. No dependency or speculative abstraction finding.

## Material findings

### I-01 — INCORRECT: whole-document callback drops retained edits to sibling panels

- Obligation: REQ-036 / AC-015, each completed edit reports the **whole updated document**; TASK-027 Scenario 3. Host decides what to keep and MCP explicitly supports a host that does not round-trip each edit.
- Exact location: `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/WyrdView.svelte:139`, with producer/consumer evidence at lines 95–110 and 134–138.
- Producer-to-consumer path: commit an edit of a, which puts its new layout in `kept.a`; the unchanged input `parsed.data` still has a's old layout. Commit a later edit of b. Rendering uses `kept.a` and `kept.b`, but `commit()` builds its callback by changing only b in `parsed.data.panels`. The second callback therefore contains the original a, even while the view displays the edited a.
- Reachability and sibling coverage: a callback that collects documents without immediately assigning `doc` is valid `WyrdView`/`mountView` usage; the shipped MCP host itself passes a no-op callback. The gallery and example round-trip promptly, which hides the defect there; `mountView` exposes no document-update method, making internal retained edits necessary for that public host as well. Both keyboard and pointer completions share `commit` and inherit the defect. All writers/readers of `kept` and `draft`, and all onlayout call sites, were inspected.
- Observable consequence: after moving two panels, the most recent callback is not the complete arrangement on screen. A host that chooses to keep that callback later restores the first panel's old arrangement. This requires no persistence feature.
- Smallest correction boundary: retain ownership in `WyrdView::commit`; construct the emitted document from all completed retained layouts plus the new edit, preserving input immutability and rendering-only collision displacement. Do not require hosts to feed each document back or change `mountView`'s public API.
- Focused closure proof: using the existing layout-editing test setup and a collecting handler, commit changes to a and b without rerendering props; assert callback two contains both changed layouts, input remains unchanged and load count unchanged. Reapplying that document should preserve the displayed arrangement. Existing tests only check the edited a and do not prove this condition.

### I-02 — INCORRECT: Escape leaves pointer edit producers active

- Obligation: REQ-036 editing includes Escape cancellation; TASK-027 Scenario 3 requires pointer move/resize with the same cancellable results as keyboard editing.
- Exact location: `crates/wyrd/wyrd-server/wyrd-ui/packages/ui/src/lib/WyrdView.svelte:153-154` and `:167-178`.
- Producer-to-consumer path: focus the move handle, start a drag on that handle, move by a grid cell, press Escape while holding the pointer, then continue moving before releasing. Escape clears only `draft`; the drag's registered `move` closure remains alive and writes a fresh draft on the next pointermove; pointerup then calls `commit`. A cancellation can consequently become a completed edit and emit the callback. Because pointerdown prevents default, an unfocused handle also does not gain focus automatically, so relying solely on its keydown is insufficient for pointer cancellation.
- Reachability and sibling coverage: this is a normal desktop interaction through the actual editable button. The resize corner runs the same pointer producer but has no key handler or focusability; a previously focused handle can receive Escape while corner dragging, with the same resumed-draft problem. Keyboard-only Escape works; pointercancel also works, but neither terminates a pointer session when the user presses Escape. Both pointer entry points, the keyboard entry, blur, commit and renderer reset were traced.
- Observable consequence: the reader presses Escape to abandon a drag but subsequent movement revives it and release emits an edit. For a focused grip, visible cancellation lasts only until the next move.
- Smallest correction boundary: cancellation must belong to the active pointer interaction in `WyrdView`; Escape must terminate its producer/listeners as well as discard its draft for both grip and corner, including when the pointer start did not focus the handle. Reuse the existing pointer cleanup/cancel mechanism; do not add a new layout framework, host responsibility or persistence state. Preserve keyboard Escape/Enter/blur and pointercancel behavior.
- Focused closure proof: focus grip, start pointer move, preview, press Escape, deliver another pointermove and pointerup; assert original layout and zero callback. Repeat for resize corner and when drag began without focused grip. Existing automated tests do not drive pointer editing; approved browser evidence covers normal drags and keyboard resize, not Escape during active drag. Add the smallest event-level regression where possible and confirm focus/capture behavior with the existing browser harness, without broad journey infrastructure.

## Verification and limits

The orchestrator independently reran the four exact TASK-027 focused commands successfully at this candidate; historical task evidence also records whole UI tests, checks/builds, codegen and human gallery approval. Those checks establish the covered paths, not the two missing negative/sequence cases above. I did not rerun or extend tests or launch a browser during this source-only independent pass. No screenshots were independently regenerated. Neither finding depends on browser layout measurement: I-01 follows callback construction, and I-02 follows live listener writes after key cancellation. Real-browser proof remains useful for pointer capture and focus closure.

Skipped implementation and unrelated TASK-003 defects. Risk: cumulative callback output and Escape cancellation remain inconsistent on reachable supported edit paths until corrected.
