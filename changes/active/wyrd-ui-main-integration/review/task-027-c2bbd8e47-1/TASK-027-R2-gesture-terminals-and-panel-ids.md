---
id: TASK-027-R2
kind: remediation
status: ready
spec: SPEC-wyrd-ui-main-integration
spec_revision: 12
parent_task: TASK-027
remediates: [FIND-027-2, FIND-027-4]
---

# Complete gesture termination and support every accepted panel ID

Execute with `$wyrd-implement` under approved revision 12.
Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Specification `changes/active/wyrd-ui-main-integration/spec.md` revision 12 approved.
Original task `changes/active/wyrd-ui-main-integration/tasks/TASK-027-view-layout.md`.
Original base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`.
Reviewed cumulative candidate `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`.
Prior candidate `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`; R1/prior ledger at `../task-027-ab1d2499f-1/`.
Current independent verdict/diagnosis: `verdict.md`, `findings-validation.md`, `followup-review.md` and `domain-review-interaction.md` beside this task.
Obligations REQ-030/036 and AC-015. UI-relative paths below start at `crates/wyrd/wyrd-server/wyrd-ui`.

## FIND-027-2 revised — terminal entries leave a held producer alive

**Diagnosis.** R1 genuinely repairs Escape: cancel invokes the renderer's release owner and removes capture/listeners. The broader producer/preview invariant still fails at other terminal entries in `packages/ui/src/lib/WyrdView.svelte:91-110,150-161,185-215,297-321`.

A held focused-grip drag writes draft. Tab/blur or Enter calls commit, which publishes and clears that draft but leaves the captured move/up producer alive. More movement recreates draft from the original pointerdown origin; release commits a second document. Fresh real Chromium against a supported non-echo mount reported y1 on blur and y2 on later release for a single held drag. Before the first movement, commit's early return also leaves the already installed producer alive. Immediate-echo gallery/example hosts can mask continued production through unrelated new-document cleanup; mountView and MCP are explicitly supported without feedback.

Width crossing below 720px or removal of onlayout makes editing unavailable and removes the captured controls while preserving panel/draft/listeners. There is no availability cancellation path, so release after removal need not reach the detached target. The preview affects narrow order/height and returns with an editing outline after widening. Fresh actual-gallery Chromium 1440→600→1440 proof retained row7 preview after pointer release instead of the previous row5. Handler removal follows the same source boundary. Original keyboard-only blur, Escape and normal steady-state mobile tests do not cover these sequences.

**Selected correction.** Keep WyrdView's existing release/stop, commit and cancel owners. Every completion ends any active pointer producer before draft/parsed early-return checks or publication, while retaining ordinary keyboard blur/Enter completion semantics. When editing becomes unavailable, invoke existing cancellation through the renderer's existing pre-update effect facility before controls disappear or removal blur can accidentally commit. Discard only the preview; preserve retained completed edits and emit nothing on availability loss. Pointerup cleanup stays safe when already stopped. Keep new-document/unmount cleanup, pointercancel and focus-independent Escape intact; do not abort/reload data for layout availability.

This single lifetime boundary covers move/corner, blur/Enter, responsive and handler transitions across all hosts. Do not suppress ordinary blur, add per-move-only availability guards, downstream callback filters, a lost-capture framework, host patches, new gesture owner or public API. These alternatives either change approved behavior or leave stale draft/listeners. The original Escape path remains closed; retain FIND-027-2 rather than create one ID per event.

**Acceptance/focused proof.**

- Non-echo host with an earlier retained sibling edit: hold drag, preview, Tab/blur; separately Enter while held. Exactly one completion; subsequent movement/up changes neither callback count nor preview. Include completion before first move: no callback then, later events inert. Exercise the corner producer through a focused grip's completion entry and verify a fresh gesture afterward.
- Hold move and resize, become narrow, release after controls disappear, then widen: no completion, no editing outline, prior effective layout and completed edits preserved. Repeat removal/reinstatement of onlayout with the existing component rerender facility. Focused/unfocused starts must not let removal blur publish an unavailable draft.
- Preserve keyboard-only Enter/blur, Escape, pointer Escape, pointercancel, fresh pointerup once, complete retained documents, immutability, no extra loads, and narrow explicit-height/read order. Use existing event tests plus bounded existing-browser capture/focus proof; no new harness.

## FIND-027-4 revised — valid IDs corrupt retained and measured state

**Diagnosis.** PanelSchema accepts arbitrary nonempty unique strings. Ordinary retained dictionaries at `WyrdView.svelte:96,107,112-120,150-155` read constructor/toString/__proto__ as inherited entries. On initial render, the inherited function/prototype overrides valid authored layout; place produces `{auto:false}` with missing coordinates. Completing an ordinary sibling similarly replaces an untouched special-ID panel layout with an inherited value, making the callback fail ViewDocumentSchema. This occurs before any editing opt-in for initial placement and affects every renderer host. It is functional corruption, not a global prototype-pollution claim.

Measurements at line 295 add a write-side failure: installed Svelte proxy get falls through inherited values, and its set trap does not establish a numeric own source for inherited names. Assigning a measured height under those names silently fails; the validator independently reproduced it with the installed runtime. Read guards alone would still leave unplaced special-ID panels without reactive occupancy. A null-prototype deep-state initializer alone also loses the installed ordinary-object proxy support. Normal-ID green tests miss both sides.

**Selected complete correction.** Preserve the existing renderer, record-based state and unrestricted schema IDs. Reuse native Object.hasOwn, already used by nearby loader dictionaries, at retained-layout selection, completed callback overlays and measurement fallbacks: inherited entries must never count as edits or measured values. Keep guards correct after empty reset and immutable spread replacement. Existing retained computed-property spread safely creates an own __proto__ edit; preserve that writer and actual completed layouts.

For measured heights, use raw immutable record replacement with computed ID properties through the existing Svelte readonly dimension function binding (null getter and setter). Retain native offsetHeight observation and teardown; replace the unsafe deep-proxy indexed write, so every actual measurement becomes a reactive own entry. Preserve actual zero measurements. The validator confirmed the installed compiler accepts this native mechanism and emits the existing bind_element_size. No new observer, map/dictionary abstraction, dependency, fallback geometry, host guard or reserved-ID validation is required. Measurements remain transient and never enter callback documents.

This fixes invalid state at its renderer-owned records rather than hiding it in place/CSS/hosts. Do not change public schemas or reject strings, or assume reads-only guards solve the measurement writer. Preserve content height, fixed row pitch, rounded occupancy, collision displacement and all original ID behavior.

**Acceptance/focused proof.** Use constructor, toString and __proto__ alongside normal siblings in existing grid/edit tests. Authored initial positions remain correct without callbacks. Editing a normal sibling preserves untouched special-ID layout or absence and produces a document that validates; edit each special-ID panel and verify normal bounded completion/cancellation. Force positive then changed offsetHeight on an unplaced special-ID panel: occupied rows and following-panel position must update reactively. That last assertion is required to reject a reads-only repair. Keep non-echo retained output, feedback/no reload, input immutability and normal IDs; no new fixture framework.

## Preserved scope and verification

Own only shared renderer state/lifecycle/binding and focused component tests necessary for both corrections. Keep FIND-027-1 and FIND-027-3 closed, original Escape behavior repaired, optional onlayout, shared pure geometry, 12 columns/40px rows/12px gaps, intrinsic auto height and <720px read-only stacking with explicit h scroll. Preserve all hosts and separate TASK-003 corrections without reverting them. No persistence, saved views, component sizing input, dependency/framework, add/remove/collapse, backend/SDK/security change or new public decision.

Apply scenario-by-scenario Red/Green using existing exact tests; private helper/fixture organization remains implementation-owned. Run these exact affected selectors after adding the closure assertions:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Pointer edit cancellation$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Grid placement$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Narrow views$'
```

Narrow final local lanes:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- git diff --check
```

Supplement with bounded existing-browser non-echo blur/Enter and breakpoint/capture sequences, retaining their callback/state evidence. No codegen, server journeys, release/Linux builds or repository aggregates are required for this unchanged schema/API write set. Record exact focused commands and expected RED/GREEN, including any newly named test's anchored selector if chosen. Stop for a real material product/API/durable decision rather than broadening remediation. Next review must audit the complete cumulative original task with prior ledgers and both R1/R2.

## Implementation Evidence

Commits `509d0f035` (FIND-027-2), `05ac2c682` (FIND-027-4) and `850cdb7a5` (test typing) on `c2bbd8e47`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Every completion ends the held producer, before draft/parse early returns | `WyrdView.svelte` `commit()` calls `release?.()` first | `TASK-027: Pointer edit cancellation`: non-echo host with a retained sibling edit; blur and Enter during held move and corner each report once, later move/up inert, sibling edit kept; completion before first move reports nothing and later events are inert; a fresh corner gesture completes once. RED before fix: preview moved after blur (`4 / span 6` vs `2 / span 6`) | PASS |
| Losing editability (narrow or no `onlayout`) cancels the preview before controls disappear, emitting nothing | `$effect.pre` cancelling when `!editable` | Same test: focused/unfocused move and corner through 1000→500→1000 px with release while narrow, and `onlayout` removed/restored via `rerender`: no callback, no `.editing`, kept layout restored. With the effect removed: stale preview `6 / span 9` vs `3 / span 9` | PASS |
| Kept edits survive a host re-passing the same document | Reset keyed on `given = $derived(doc)` rather than the reparsed result | `rerender({ onlayout })` keeps completed edits (earlier failed: `1 / span 6` vs `3 / span 9`) | PASS |
| `constructor`/`toString`/`__proto__` IDs place, edit and validate like normal IDs | `Object.hasOwn` at retained layout selection, callback overlay and measured-height read | `TASK-027: Grid placement`: special-ID document places identically to a renamed one. `TASK-027: Layout editing`: editing a normal sibling keeps special layouts (and absence) and validates; each special ID completes and cancels; final document validates with own `__proto__.layout` | PASS |
| Measured heights become reactive own entries for any ID | `heights` is `$state.raw` replaced whole via `bind:offsetHeight={null, setter}` | Grid placement: unplaced `__proto__` at 294 px → follower `7 / span 4`; after 400 px + resize notify → `1 / span 8` and `9 / span 4`. A reads-only repair (deep proxy write) fails: `5 / span 4` vs `7 / span 4` | PASS |

Test support: `vitest-setup.ts`'s existing ResizeObserver stub now records its observers and `ResizeObserver.notify()` reports them, so jsdom can drive a width change mid-gesture. It is not a new harness.

Browser proof: Playwright's installed headless Chromium 1248, driven over CDP with trusted mouse and key events against the dev gallery page. A non-echo `mountView` host contained ids `a`, `constructor`, `__proto__` (unplaced) and `after`. The Chrome extension browser could not reach the local server.

| Sequence | Callbacks | Observation |
|---|---|---|
| Keyboard sibling edit (`constructor`) | 0→1 | — |
| Held move/corner + Tab, then move 220 px + release | +1 each | Row unchanged after release, no `.editing`, each callback carries sibling `1/6` |
| Held move/corner + Enter, then move + release | +1 each | Same |
| Tab before any move, then move + release | +0 | Rows unchanged |
| Held focused move / unfocused corner, 1200→600 px, release while narrow, →1200 px | +0 | Grips 4→0→4; rows return to kept `3 / span 8`; no `.editing` |
| `__proto__` content set to 400 px | +0 | `__proto__` `11 / span 5`→`11 / span 9`, follower `16`→`20` |

Commands (all exit 0, macOS arm64):

- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'`, `'^TASK-027: Pointer edit cancellation$'`, `'^TASK-027: Grid placement$'`, `'^TASK-027: Narrow views$'`: each 1 passed, 9 skipped.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts`: 10 passed.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check`: 0 errors, 0 warnings. `build`: built. Example app `--filter @wyrd/example-ui-app check`: 0 errors.
- `mise exec -- git diff --check`: clean.

Schemas, public API, the geometry module and hosts are unchanged; no dependency was added. The browser cannot remove `onlayout` from a static `mountView`, so that path is proved in jsdom only. Limit: macOS arm64 only.
