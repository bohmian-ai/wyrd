# TASK-027 — interaction, accessibility and geometry domain review

Result: **FAIL**. The three prior findings close on their recorded paths. Two additional reachable transitions leave the active pointer producer alive beyond its interaction boundary.

## Immutable subject and scope

Reviewed original base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb` to candidate `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`, including original TASK-027 and TASK-027-R1. HEAD was confirmed at that candidate. This report covers the complete TASK-027 layout contract, renderer, editing lifecycle and actual gallery/example/MCP consumers; unrelated cumulative changes and the independent TASK-003 ledger are not attributed to TASK-027. No current-attempt peer report was consulted. No source or test file was changed and no harness file was created.

## Authority and source coverage

| Authority | Application and inspected source |
| --- | --- |
| Supplied AGENTS.md; `architecture/agent-rules.md`; `architecture/references/README.md`; spec-driven-development and maintainer-style references | Immutable acceptance audit, smallest existing owner, scoped proof and source-backed findings. No `.codegraph` exists, so ordinary source navigation was used. |
| Approved revision 12 spec REQ-030, REQ-036 and AC-015; original TASK-027; prior validated FIND-027-1..3 and R1 | Twelve columns, 40px rows, collision push-down, intrinsic height, immutable whole-document callbacks, optional editing, pointer/keyboard cancellation, responsive read-only behavior and render-local host state. |
| `.agents/skills/wyrd-ui/SKILL.md`, Svelte 5 and testing-verification references; `brand/DESIGN.md` | Semantic keyboard handle, visible focus, mode tokens, responsive layout and cleanup. Broader Wyrd design/doctrine remains the product boundary; this change introduces no durable service authority. |
| `packages/ui/src/lib/schemas.ts:179-205`; catalog generation and published view schema | Layout bounds and strict rejection; component schemas do not receive sizing inputs. |
| Complete `packages/ui/src/lib/layout.ts` and `WyrdView.svelte` | Geometry producer and all renderer consumers; authored/retained/draft separation, measurement, ordering, key/drag/commit/cancel, load identity, teardown, containment and CSS. |
| `packages/ui/src/lib/view.ts`, `packages/ui/src/app/main.ts`; gallery `+page.svelte`; `examples/ui-app/src/panels.ts` and `/view/+page.svelte` | Non-echo mountView; echoing gallery/example hosts; MCP no-op callback and destroy/remount on each tool result. |
| Chart, CardHeader and EmptyState component consumers; `src/lib/ui-library.test.ts:586-773` | Intrinsic/fixed size consumers, loading-to-loaded behavior, schema/grid/edit/narrow tests and new pointer cancellation test. |

The existing renderer remains the correct owner: it already owns DOM measurement, retained edit state and active pointer cleanup. Pure placement remains in the small existing geometry module. No additional store, host merge, dependency, layout framework or API is necessary for either correction below.

## Requirement and path assessment

| Boundary | Source and proof | Result |
| --- | --- | --- |
| Layout validation, schema publication, no component size inputs | Strict bounded LayoutSchema; source-driven catalog; focused document-contract check green in `verification.md` | PASS |
| Fixed row pitch, measured intrinsic occupancy, later collision push-down, actual auto height | `rowsFor`, `place`, fixed 40px desktop tracks and `align-self:start`; `bind:offsetHeight` observes changing content/width. Pure and measured DOM tests plus supplied real-browser physical measurements | PASS |
| Retained/authored documents, multiple sibling edits, untouched auto/collision state | `commit` overlays all `kept` entries only; callback does not serialize placement/height; non-echo regression and reapply proof | PASS |
| Keyboard arrows, Shift resize, Enter/blur completion, Escape rollback and focus visibility | Shared `edit`, keyboard handler, native labeled button, `:focus-visible`; ordinary keyboard paths supported | PASS for isolated keyboard edits; see INTERACTION-027-1 for blur during held pointer |
| Pointer movement/resize snapping, ordinary completion, Escape regardless of focus, pointercancel | Shared `edit`; capture/listener owner `release`; window Escape invokes `cancel`. Focused/unfocused move and unfocused corner freshly exercised in Chromium using the existing `escape.mjs` probe | PASS for the recorded prior finding; terminal transitions below FAIL |
| Unmount and new document/tool-result replacement | Teardown releases gesture; parsed-input pre-effect calls cancel and clears kept. MCP destroys old mount before a new tool result; no persisted state | PASS by source tracing |
| Narrow initial render, `(y,x)` reading order, full width and retained explicit h | Measured view width drives `narrow`; ordered DOM; boxStyle removes grid row and retains explicit height; narrow test and supplied 375px browser evidence | PASS for steady state; crossing breakpoint during drag FAIL |
| Containment and chart redraw | `.fill` min sizes/overflow contain catalog content; Chart measures available box and retains readable plot floor; mobile explicit h remains scrollable | PASS within provided physical proof |
| Host callback/load invariants | `content` excludes layout, loader derives from `load ?? executor`; gallery echoes and MCP does not; built example client memoizes inline `remoteLoader('/view/load')` in a dependency-free derived value, so doc echo does not rebuild it | PASS apart from duplicate completion below |

## Prior finding closure

- **FIND-027-1 closed:** WyrdView lines 150-155 builds `done` from every retained edit and overlays it on the authored panels. The two-sibling non-echo test checks complete output, untouched unplaced/colliding siblings, immutability and no reload after reapplication.
- **FIND-027-2 closed on its diagnosed path:** lines 159-161 and 185-214 now remove the actual move/up/cancel/window-key listeners and release capture. Fresh existing `escape.mjs` execution at 1440px restored `1 / span 8 | 5 / span 8` after previews in all three focus/start combinations; further moves/up stayed inert. Fresh resize then completed at `h=9`. The remaining findings are distinct terminal transitions, not a claim that the direct Escape fix failed.
- **FIND-027-3 closed:** occupancy is measured continuously, rounded only for row reservation, and fed to the pure placement owner. Actual auto content keeps intrinsic height inside fixed tracks; edit-start reads that placement; callbacks serialize only authored edits. The supplied R1 browser evidence covers short header at y4/top208, tall chart push-down to y6/top312, loading settlement, width changes and mobile. Current source supports those measurements without a second offset authority.

## Material proposed findings

### INTERACTION-027-1 — blur commits a held pointer draft but leaves it producing a second completion

Classification: **INCORRECT**. Obligation: REQ-036/AC-015 and original scenario 3 require one whole-document notification per completed change; R1 requires the existing pointer lifecycle and keyboard blur behavior to remain coherent.

Location: `packages/ui/src/lib/WyrdView.svelte:150-155`, `185-214`, `305`.

Producer-to-consumer path: drag installs capture plus move/up listeners and stores their cleanup in `release`. Pointermove produces `draft`. The move button's blur invokes `commit`, which publishes that draft and clears it but never stops `release`. Further captured movement creates a new draft from the same pointerdown origin; pointerup invokes a second commit. `commit` is also reachable by Enter during that held gesture. This is shared renderer behavior for both pointer handles and every host; the gallery's echoed doc can mask continued production by triggering the separate new-doc cancellation, whereas mountView and the MCP App intentionally do not echo.

Fresh real Chromium proof used the existing 5180 gallery's built `mountView`, existing `fixtureExecutor` and timeseries fixture, mounted into a temporary DOM host with a recording non-echo callback. Viewport 1200x1000; panel `{x:0,y:0,w:8,h:6}`; focus grip; pointerdown; move 52px; press Tab while held; move to 104px; pointerup:

```text
before: row 1/span6, reports []
first move: row 2/span6, reports []
Tab/blur: row 2/span6, reports [{x:0,y:1,w:8,h:6}]
continued move/up: row 3/span6, reports [{x:0,y:1,w:8,h:6},{x:0,y:2,w:8,h:6}]
```

Observable consequence: one held drag emits two completed documents. On the echoing gallery, Tab instead commits the preview early, so subsequent Escape cannot restore the pre-drag layout. This is an ordinary focus transition, not an invalid document or synthetic pointer-only event sequence.

Smallest correction boundary: keep `commit` as the one completion owner and terminate its existing active pointer producer as part of completing the draft. Preserve keyboard blur completion, pointerup exactly once, retained earlier edits and direct Escape cancellation. Do not add host guards or a second gesture implementation. Focused closure: repeat the real event sequence against a non-echo host and prove exactly one completion; later movement/up cannot produce a second document after completion. Preserve ordinary keyboard blur and fresh subsequent pointer gestures.

### INTERACTION-027-2 — crossing into read-only narrow mode strands an active pointer preview

Classification: **INCORRECT**. Obligation: REQ-036 narrow views turn resize/move off; R1 preserves `<720px` ordered read-only stacking and the renderer-owned active pointer lifecycle.

Location: `packages/ui/src/lib/WyrdView.svelte:91-110`, `185-214`, `297-321`.

Producer-to-consumer path: `editable` becoming false removes the captured grip/corner. The gesture listeners remain attached to that removed element and its window Escape listener remains installed; neither the width/editability transition nor capture loss invokes `cancel`. The `draft` therefore continues to override kept/authored layout even though no editing controls exist. Pointer release after removal no longer reaches the removed handle's pointerup completion. On widening, the same draft and editing outline return without a held pointer.

Fresh real Chromium proof in the actual light gallery, starting with the grip unfocused at 1440x1000: pointerdown on latency grip; move 104px; resize viewport to 600x1000; release pointer; restore 1440x1000:

```text
before: row 5/span8, editing=false, handles=1
held preview: row 7/span8, editing=true, handles=1
600px: grid row unset, editing=true, handles=0
release then 1440px: row 7/span8, editing=true, handles=1
```

Observable consequence: a gesture whose controls disappeared and whose pointer was released leaves an uncompleted preview applied indefinitely, including to mobile reading order. Returning to desktop resumes a stale edit state. Handler removal has the same source boundary (`editable` false) although only the width transition was physically probed.

Smallest correction boundary: invoke the renderer's existing cancellation owner when editing becomes unavailable, ending capture/listeners and discarding only the active preview. Preserve completed edits and authored mobile height/order; no host-specific cleanup or new API. Closure: active move and resize across the breakpoint, release after controls disappear, then widen; no callback and the previous completed layout remains, with no editing outline and a fresh gesture still functional.

## Proof limits

The root's current `verification.md` records all nine focused checks, UI/example check/build and codegen green; those checks do not cover the two new event sequences. I freshly reran the existing real-browser Escape probe and used the installed Playwright/Chromium with the existing served gallery/mountView fixtures for the two bounded event probes; no test harness was added. Physical geometry measurements and broad light/dark/mobile artifact evidence remain the supplied R1/original evidence rather than fresh reruns. No Linux, live MCP host, real backend journey or full aggregate was run by this reviewer. Accessibility assessment is source plus keyboard/focus interaction, not a screen-reader certification.
