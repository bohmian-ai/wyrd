---
id: TASK-027-R1
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 12
parent_task: TASK-027
remediates: [FIND-027-1, FIND-027-2, FIND-027-3]
---

# Correct retained layout output, gesture cancellation and intrinsic occupancy

Execute with `$wyrd-implement` under approved revision 12. This is bounded renderer remediation.

Specification: `changes/active/wyrd-ui-main-integration/spec.md`, revision 12 approved.
Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-027-view-layout.md`.
Review base: `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`.
Candidate: `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`.
Verdict/validated ledger: `verdict.md` / `findings-validation.md` beside this task.
Obligations: REQ-036, AC-015 and REQ-030 view contract.

UI paths below are rooted at `crates/wyrd/wyrd-server/wyrd-ui`.

## FIND-027-1 — callback document omits earlier completed sibling edits

**Diagnosis.** `packages/ui/src/lib/WyrdView.svelte:134-139` records every completion in kept, but callback construction overlays only the current panel on parsed input. Rendering at lines 95-111 uses every retained entry. Edit A then B without host feedback: display keeps both edits while callback two contains original A plus edited B. Applying that latest document resets retained state and loses A. `mountView` permits observing callbacks and exposes no update method; the shipped MCP host intentionally uses a no-op handler. This is required non-feedback behavior, not a request for persistence. Gallery/example immediate feedback masks it; the current test edits only one panel before feedback.

**Selected outcome/correction.** Keep ownership in WyrdView and use its existing retained authored edit state to construct complete callback documents. Include all previous completed edits plus the current one, preserving original fields/order and untouched panel representation, including no layout. Do not serialize resolved placements: collision push-down and measured occupancy remain render-only. All Enter, blur and pointer-up completions already share this owner; no host merges, mandatory feedback protocol, new store or API are needed. New authored input still resets local state; input immutability and load identity stay intact.

**Closure.** With a recording host that does not echo props, complete edits to two different panels; callback two contains both edits and fires once for the second completion. Reapply it without lost edits/reloads. Original input remains unchanged. Include an untouched colliding/unplaced sibling to prove collision and measurement results are not authored into the callback.

## FIND-027-2 — Escape fails to terminate pointer edit production

**Diagnosis.** `WyrdView.svelte:145-178` clears draft on Escape but leaves drag's move/up listeners alive. Focus grip, pointerdown, move, Escape, move again, release: the captured move recreates draft and pointerup commits it. Pointercancel cleans up, but Escape does not reach that lifecycle. Corner resize shares drag and lacks its own keyboard cancellation path; pointerdown prevents focus, so an unfocused start cannot depend on grip keydown. Existing proof covers keyboard-only Escape and successful drags, not this sequence.

**Selected outcome/correction.** Extend the existing renderer-owned active pointer lifecycle so Escape terminates producers/listeners and capture as appropriate, discards preview and cannot later commit. Escape must reach move and resize regardless of starting focus. Reuse existing cancel/end responsibility; no gesture library, per-host guard or second layout state owner. Preserve earlier completed edits, keyboard Enter/blur and Escape, normal pointer-up once per completion, pointercancel and no reloads.

**Closure.** For move and resize, start gesture, change preview, Escape, further move, release; original effective layout restored and no callback. Cover focused and unfocused starts. A subsequent fresh gesture must still complete once. Retain keyboard blur/Enter and pointercancel behavior. Use existing component event facilities plus bounded real-browser focus/capture confirmation; no new dependency or full host-journey suite.

## FIND-027-3 — intrinsic panels stretch the coordinate grid

**Diagnosis.** `packages/ui/src/lib/layout.ts:28-35` models all unplaced panels as h=1. `WyrdView.svelte:108-130,164-168,310-315` renders that intrinsic panel across one minimum-height track; CSS minmax(40px,auto) grows it while rowsHeight and pointer movement retain 52px pitch (40px row + 12px gutter). current measures intrinsic height only when editing begins. The ordinary placement producer therefore lacks actual occupied extent.

Focused AppBridge measurements in followup-review.md corroborate the source: a 72.1875px intrinsic header before a fixed y=4 panel moves its top to 240.1875px instead of 208px. Converting that earlier header to a two-row fixed panel moves the unchanged later panel to 208px. A 294px intrinsic Chart similarly expands one row. CSS currently preserves separation by stretching; overlap is not claimed from these measurements. On a repaired fixed grid the 294px Chart genuinely collides with y=4 and must push that later panel down; do not require 208px in that tall-content case. Existing DOM tests assert style strings and cannot measure tracks.

**Selected outcome/correction.** Keep WyrdView's existing DOM measurement responsibility and layout.ts's pure placement/collision owner. Measure intrinsic content's occupied whole-row extent and feed that transient extent into placement; desktop tracks remain fixed at 40px. Reserve enough rows for actual content and gutter geometry, while retaining the auto panel's actual content height rather than stretching its visible box to rounded occupancy. Actual height and occupied rows are distinct from authored Layout; neither measurement nor collision output may mutate documents or leak into callbacks. Auto content is not restricted by the explicit-layout h≤48 schema bound.

Keep occupancy current when available width or asynchronously loaded content changes so initial loading cannot recreate the mismatch. Placement/collision, edit-start and responsive consumers share this existing authority; do not add independent offsets, component size props, a replacement renderer, dependency, store or public API. Fixed CSS rows alone would sacrifice intrinsic height, and minimum auto rows would retain wrong pitch; both incomplete alternatives are rejected. Preserve mobile fixed-h stacking/scrolling and all-unplaced content behavior.

**Closure.** Browser proof with an intrinsic header short enough not to collide at y=4: later fixed panel starts exactly at 208px. With a tall intrinsic Chart, preserve intrinsic content height and push colliding panel to first clear whole row, no overlap. Computed desktop row tracks stay 40px; input document unchanged; converting auto to fixed does not redefine pitch. Include content settling after initial loading, width change, all-unplaced documents, and existing mobile behavior. Pure placement/collision assertions supplement physical browser measurements; style strings alone cannot close this defect.

## Constraints, scope and preserved behavior

Own WyrdView editing/measurement and pure layout geometry, their focused tests, and directly affected shared consumers only as necessary. Existing schema/catalog, mountView, gallery/example and MCP contracts stay the same. Keep generated outputs source-driven. Coordinate shared WyrdView changes with TASK-003-R1 rather than reverting them; its chart/action/error fixes are a separate ledger.

Preserve document immutability, render-only collision resolution, no reload on layout-only updates or same executor, optional onlayout, 12 columns, 40px desktop rows, 12px gutter, <720px ordered read-only stacking and fixed-h internal scroll. MCP edits remain render-local and a new tool result destroys them normally. No persistence, saved views/per-viewer memory, add/remove/duplicate/collapse, component size input, framework, dependency, production MCP registration or backend change. No new product or public API decision is authorized; escalate a real authority conflict rather than extending scope.

## Ordered proof and acceptance

Apply scenario-by-scenario Red/Green in the existing focused tests: callback completeness, active gesture cancellation, then measured intrinsic occupancy. Record expected failure, minimal passing correction and regressions. Acceptance is the observable closure for each FIND above, preserving the original task's other obligations. Test fixtures/private code structure remain implementer-owned.

Run exact affected scenarios through the pinned toolchain:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Grid placement$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout document contract$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Narrow views$'
```

Narrow final coverage for this renderer write set:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- git diff --check
```

Use the existing gallery/AppBridge browser capability for the bounded focus/capture and mixed-grid checks. Record actual viewport, fixture/action and measured rows/boxes; followup-review.md identifies the existing artifact proof, not a new test harness to build. If authored schema/catalog changes become necessary, justify within approved behavior and run mise run codegen:check; none is currently required by the selected correction. Do not require repository aggregates, release/Linux builds or full real-server journey suites. Later task review reassesses the complete cumulative original task and these stable FIND IDs.

## Implementation evidence

Commits: `b512c2221` (FIND-027-1/2), `84d5dd22e` (FIND-027-3).

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| FIND-027-1 every completed edit is reported against a non-echoing host | `WyrdView.svelte` `kept`/`draft` as `$state.raw`; `commit()` reports `{...kept, [draft.id]: draft.layout}` | `TASK-027: Layout editing` (two edits, second callback carries both; unplaced stays unplaced; no reload) | PASS |
| FIND-027-2 Escape ends a held pointer gesture; later moves/release change nothing | `WyrdView.svelte` `drag()` registers `release` (listeners + pointer capture), `cancel()` calls it; window Escape listener; teardown effect | `TASK-027: Pointer edit cancellation`; browser `escape.mjs` at 1440: grip focused/unfocused and corner unfocused all restore on Escape, release is inert, fresh resize commits h 8→9 | PASS |
| FIND-027-3 auto panels occupy `rowsFor(height)` rows; row pitch fixed at 40px + 12px | `layout.ts` `rowsFor`, `place(layouts, heights)`; `WyrdView.svelte` `bind:offsetHeight`, `align-self: start`, `grid-auto-rows: 40px` (auto when narrow) | `TASK-027: Grid placement` (72.19→2 rows no push; 294→6 pushes y4 to 6; 0/2000→1/39; DOM spans); browser `geometry.mjs`/`settle.mjs` | PASS |

Browser measurements (Playwright, 1200px light and dark): tracks 40px; 72.19px header → 2 rows, later panel top 208; tall 298px chart → 6 rows, later panel row 7 top 312; loading→loaded 5→6 rows; width 740 header 3→4 rows; every top a multiple of 52, no overlap, no horizontal overflow; 375px stacks at content height.

RED observed before each fix: second callback missing the earlier edit; gesture revived after Escape; auto `h` stayed 1 instead of 2.

Commands (all green):

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Grid placement$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout document contract$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Narrow views$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Pointer edit cancellation$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts   # 10 passed
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check                             # 0 errors, 0 warnings
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
git diff --check
```

Non-goals held: layout document contract unchanged, catalog components stay size-agnostic, no new dependency. Limit: proof ran on macOS arm64 only.
