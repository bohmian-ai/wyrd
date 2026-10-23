# TASK-027 interaction and reactive lifecycle domain review

Overall result: **FAIL**. One new material proposal, **INTERACTION-1**, requires independent validation. Prior FIND-027-1 through FIND-027-5 are closed for their diagnosed causes.

## Subject, authority and scope

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; cumulative base `5e57416d562386066c1aadda9dd01783b9cd4eb5`; candidate `7579a2f603c023daae0d15b9d5cc3013eec6f00d`. HEAD remained candidate. Reviewed actual cumulative diff, original `tasks/TASK-027-view-layout.md`, revision-13 `spec.md` REQ-030/036 and AC-015, prior TASK-027 verdicts and R3 diagnosis, and current source/tests. This is independent discovery; no current peer report was read.

Applied root AGENTS.md, architecture authority map and relevant client/presentation doctrine, `changes/spec-driven-development.md`, `agent_docs/index.md` and maintainer style, TESTING.md, Wyrd UI skill and its Svelte/testing references, and brand/DESIGN.md. No CodeGraph directory exists. No additional nested rules apply to these UI/example paths. The boundary is transient browser interaction, reactive host input and responsive presentation; server persistence, security, tenancy and unrelated PR #205 content are outside it.

## Coverage and ownership

UI-relative paths begin at `crates/wyrd/wyrd-server/wyrd-ui`.

| Boundary | Source inspected and traced |
|---|---|
| Authored input | `packages/ui/src/lib/schemas.ts` strict layout/panel/view schemas; complete `WyrdView.svelte` parser, content and authored value derivations. Direct Svelte use is explicitly published by REQ-030. |
| Local layout lifetime | `WyrdView.svelte:91-191`: kept/draft, own-entry reads, raw height record, replacement/value reset, availability cancellation, complete commit/cancel/key bodies. |
| Native interaction | `WyrdView.svelte:198-229`, template grip/corner and styles: capture, move/up/cancel, window Escape, listener teardown, focus/blur/Enter, unmount release, named native button, focus ring, keyboard corner equivalent. |
| Responsive projection | `layout.ts` complete geometry; `WyrdView.svelte:121-155` document placement, loaded-panel ordering, styles and current edit origin; narrow styles and measured widths/heights. |
| Data generation seam | `WyrdView.svelte:73-89,235-269`: content/source trigger, start, abortAll and settle; exact writers of `panels` are start and settle. Parsed document drives placements independently of those writes. |
| Hosts and consumers | Complete `view.ts` mount/destroy/options, `src/app/main.ts` tool-result remount and per-render local lifetime; gallery and example /view callback echo and document replacement; exported renderer direct-host path. |
| Proof | TASK-027 contract/grid/editing/pointer-cancellation/narrow tests, related TASK-003 host/loading behavior, `ui-fixtures.svelte.ts`, existing observer/capture support, prior native-browser/focus/capture evidence. New installed-runtime probe below tests a path absent from existing tests. |

The existing owner is WyrdView. Geometry is a pure helper reused by pointer/keyboard and rendering. No duplicate persistent layout store, parser, grid dependency or lifecycle framework was added. The one new proposal concerns the existing presentation join of document geometry and asynchronously refreshed loaded panels; its correction belongs at that join, not in a second loader or global lifecycle.

## Domain acceptance and prior finding closure

| Obligation or prior finding | Source and verification evidence | Result |
|---|---|---|
| Whole callback retains prior completed sibling edits, input unchanged | commit overlays every own retained entry on parsed panels; non-echo sibling test preserves untouched auto/colliding siblings and authored input | PASS; FIND-027-1 closed |
| Every completion/cancellation ends held pointer producer | commit releases before its early return; cancel releases and drops draft; stop removes move/up/cancel/window listeners and capture; pre-effects cover availability/document loss and teardown covers unmount. Existing focused/unfocused grip/corner, Enter/blur/breakpoint/handler tests and prior native Chromium evidence support these exact branches | PASS; FIND-027-2, original and revised causes, closed |
| Fixed pitch and intrinsic occupancy | fixed CSS 40px tracks, rowsFor/place/rowsHeight share constants, native offsetHeight writes preserve intrinsic occupancy, fill contains overflow | PASS; FIND-027-3 closed, existing geometry tests and prior physical browser evidence |
| Arbitrary accepted panel IDs | own retained/height reads, raw immutable computed-property record writes and Map placements; constructor/toString/__proto__ validation/edit/cancel/changed-height coverage | PASS; FIND-027-4 closed |
| Nested reactive authored changes supersede retained edits and cancel held producers | authored observes stable full parsed JSON beside given reference; reset invokes existing cancel and clears kept. Separate layout-stripped content remains load authority | PASS; FIND-027-5 closed. Native Svelte proxy regression assertions cover authored y7, callback preservation, no layout-only reads, range/input during held move/corner, unchanged handler toggles and equal-content new reference |
| Keyboard/focus and read-only interaction | named native grip, Arrow/Shift+Arrow shared edit clamping, Enter/blur commit, Escape cancel, visible focus ring; controls absent without handler or below breakpoint | PASS for inspected and existing tested paths |
| Steady narrow layout | width-based breakpoint, full-width box style retaining explicit h, y/x sort; exact existing narrow scenario | PASS in steady state |
| Narrow layout after host loads another valid document | new parsed placements and previous loaded panels can coexist during reactive render; comparator dereferences missing placement | FAIL; INTERACTION-1 |
| Per-render local state, no reload from edits, no persistence | mount/destroy host wiring, layout-stripped content and stable source, kept local to renderer; no saving/local storage/extra APIs/dependencies | PASS |

FIND-027-5 is not reopened: its authored reset now observes the required state and performs correct cleanup. The new failure is in the older narrow rendering join, which never had an invariant guaranteeing that parsed panel identity and loaded panel identity switch in the same reactive phase.

## INTERACTION-1 — Narrow document replacement crashes while loaded panel IDs belong to the previous document

- **Classification:** REGRESSION / INCORRECT.
- **Obligation:** REQ-030's supported custom Svelte renderer and REQ-036/AC-015 narrow full-width rendering; reloading a document must show its authored layout. Valid host document replacement is existing public renderer behavior, not adding/removing panels through layout controls (the latter remains a non-goal).
- **Exact location:** `packages/ui/src/lib/WyrdView.svelte:137-138`, `ordered` comparator. Producer seam: document-driven placements at `129-134`, post-render load effect at `83-89`, and `start` replacing panels at `241` onward.
- **Reachable path:** render a valid two-panel document at measured width 500px in a direct Svelte host. Replace it with another valid two-panel document using different IDs. Parsed/placements update immediately from the new host input; the existing loaded `panels` array still contains the old IDs until the load effect calls start. `ordered` sorts the old panels and obtains undefined from placements for those IDs, then reads `p.y`/`q.y`. The new authored reset correctly runs, but it does not synchronize these separate producers and should not be made to own loading.
- **Observable consequence:** an uncaught Svelte render TypeError interrupts the narrow renderer during an ordinary valid document reload. At desktop width the same input transition completes. This occurs with no editing handler and no pointer activity, so no gesture event or untrusted callback is needed.
- **Writer/sibling trace:** the host writes doc; parsed supplies layouts, content, authored and placements. Only start replaces `panels`; settle mutates one current panel's result and respects aborted controllers. Width changes affect narrow/order. Ordered feeds the keyed panel loop; boxStyle already checks a missing placement, but ordering crashes before it can help. `current` is used only by available edit controls and is not involved in this read-only narrow reproduction. mountView and MCP remount static inputs and therefore avoid this particular transition, while the exported WyrdView and direct Svelte host accept it. Gallery/example echo layouts using stable IDs, which explains why existing host tests miss it.
- **Minimum correction boundary:** keep the existing data-generation lifecycle and cancellation semantics. Make the existing narrow render projection join only current document panel identities with available loaded panel rows before reading placement values, so an older generation cannot enter the comparator. A local exclusion of rows without a current placement at this join is appropriate: the renderer itself owns the cross-generation presentation invariant and these old rows are no longer in the current document. Do not move loading into a pre-effect, create a second loader, synchronize a new global store, retain obsolete panel geometry or invent fallback positions for removed rows. Keep document-order tie behavior and y/x ordering of current rows.
- **Focused closure proof:** add an ordinary component regression to the existing narrow scenario: with at least two loaded valid Chart panels at 500px, rerender a different valid document with different IDs and flush. Assert no unhandled render exception, only new IDs, correct y/x order and retained explicit height. The same producer mismatch can be covered by a native proxy panel-ID change using existing facilities if useful. Preserve completed reactive edits/terminal tests and no layout-only reload behavior. Existing exact narrow selector plus affected editing/cancellation selectors and UI check/build suffice; no new harness, browser driver or server journey is needed.

## Fresh runtime evidence

I ran an in-memory source probe through `mise exec -- node --conditions=browser --input-type=module` from the UI directory. Node was v26.2.0, installed Svelte runtime/compiler 5.57.2. `registerHooks` compiled repository `.svelte` files to client modules with installed `svelte/compiler`, stripped `.ts` types, and ignored CSS; jsdom supplied DOM globals, clientWidth=500 and inert ResizeObserver. No file or generated output was written. A tiny compileModule `$state({doc:undefined})` created the actual native Svelte reactive proxy; a getter prop passed its document to the real unmodified WyrdView. This tests the installed runtime ordering, not a hand-simulated comparator.

The behavioral core was:

```js
const input = { type: 'stat', title: 'Count', sql: 'SELECT count(*) AS count FROM t', y: ['count'] };
const doc = ids => ({
  version: 'wyrd.view/v1', catalogId: 'wyrd.ui/v1',
  range: { from: '2026-10-20T00:00:00Z', to: '2026-10-21T00:00:00Z' },
  panels: ids.map((id, i) => ({ id, component: 'Chart', input, layout: { x: 0, y: i * 5, w: 12, h: 4 } }))
});
proxy.doc = doc(['a', 'b']);
mount(WyrdView, { target: document.body, props: {
  get doc() { return proxy.doc; },
  load: async () => ({ input, result: { state: 'empty' } })
} });
await tick();
proxy.doc = doc(['c', 'd']);
await tick();
```

The source validator confirmed `ViewDocumentSchema.safeParse(doc(['a','b'])).success === true` and `validatePanel('Chart', input)` had no code. The same document builder produces c/d. At 500px the process printed `valid document true valid panel true`, `before [ 'a', 'b' ]`, then exited 1 with `TypeError: Cannot read properties of undefined (reading 'y')` at the compiled `return p.y - q.y || p.x - q.x`, followed by `Proxy.toSorted`, Svelte `execute_derived`, `is_dirty` and batch traversal. Compiled line numbers differ from source; the source expression is line 138. A second run changing only clientWidth to 1000 exited 0 and printed `after [ 'c', 'd' ]`. A preliminary probe with incomplete stat input also failed, but the retained evidence above repeats the failure with fully validated ordinary component input.

## Verification limits

Candidate-integrated check/test/build and live Postgres 2/2 results are supplied evidence, not fresh executions by this reviewer. Existing tests do not replace panel identities under narrow layout, so those green results do not contradict the fresh failure. Historical native pointer/focus/capture/geometry and human acceptance evidence was inspected; no fresh real-browser capture or visual sweep was run. The new failure is proved in the installed client runtime and requires neither actual layout geometry nor a backend; simulated width is sufficient to choose the failing branch.

Only this report was written. No source/test/generated files, commits or pushes were changed. Skipped broad suites and fresh native-browser execution; risk remains the demonstrated narrow document-replacement crash pending independent validation and remediation.
