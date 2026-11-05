# TASK-027 cumulative system-resilience re-review

Result: **PASS**. No material system-resilience finding established. Reviewed source remains unchanged.

## Subject and coverage

Repository: `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`.
Original base: `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`.
Candidate: `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`; HEAD reconfirmed during this review.

Reviewed the cumulative base-to-candidate TASK-027 implementation, not merely R1: `schemas.ts`, generated catalog projection, `layout.ts`, `WyrdView.svelte`, `view.ts`, `src/app/main.ts`, gallery, example document/page and focused tests. Used the latest `ab1d2499f...c2bbd8e47` diff to locate remediation, then traced complete owners and callers. Unrelated Rust/build changes in the cumulative range are not attributed to TASK-027. Shared loader/error changes belong to TASK-003; their interaction with layout and renderer failure isolation was inspected here.

Authority read: supplied AGENTS.md, `architecture/agent-rules.md`, reference router, spec-driven-development and maintainer-style references; Wyrd design/doctrine client and server ownership; approved revision-12 spec REQ-030/036 and AC-015; original TASK-027; prior verdict/validated FIND ledger and R1 task/evidence; task-review role instructions; Wyrd UI skill and its Svelte/verification guidance. No `.codegraph` exists. This is a renderer lifecycle/recovery audit; no new backend availability or durability obligation is inferred.

## Deployed ownership and consumers

UI paths in this report are relative to `crates/wyrd/wyrd-server/wyrd-ui` unless identified otherwise.

| Path | Source and owner evidence | System effect |
|---|---|---|
| Direct Svelte and gallery | `packages/ui/src/lib/WyrdView.svelte:45-155`; `src/routes/_gallery/+page.svelte:37-51,102` | Renderer retains authored completions and previews locally; gallery feeds complete callback documents back into its page-local document. No persistence or transport added. |
| Generic DOM mount | `packages/ui/src/lib/view.ts:27-47` | `mountView` forwards the existing props including optional `onlayout`; destroy unmounts the same renderer. Recording callbacks need not echo documents. |
| Example remote host | `examples/ui-app/src/routes/view/+page.svelte:2-13`; `packages/ui/src/lib/view.ts:54-65` | Page holds transient document; remoteLoader transmits component/input/range with AbortSignal, never panel layout. The existing BFF/server read boundary remains the data owner. |
| MCP App artifact | `packages/ui/src/app/main.ts:15-27`; `packages/ui/src/lib/mcp.ts:39-64` | Each new tool result destroys the old mount and mounts a fresh document. No-op layout callback intentionally leaves edits in renderer state for this render; host holds credentials and tool transport. |
| Geometry and intrinsic components | `layout.ts:22-52`; `WyrdView.svelte:112-142,286-321,350-378`; Chart's box measurements and CardHeader content | Pure placement receives measured occupancy; renderer owns DOM width/height and containment; components remain size-agnostic. No second renderer, gesture framework, loader pipeline or saved-view authority introduced. |

The browser/Svelte component is the relevant failure boundary. TASK-027 adds no service process, shared server fatal path, persistent data, retry loop, health check or recovery queue.

## Failure, interruption and recovery paths

| Event/path | Producer → consumer and affected state | Recovery/proof assessment |
|---|---|---|
| Multiple edits without host feedback | `commit` (`WyrdView:150-155`) merges current draft into all retained authored completions before constructing the callback; `layouts` (:112-114) uses the same retained state | Screen and whole-document output agree. Echoing the latest output resets local overrides to the equivalent authored document, preserving earlier edits. Focused Layout editing assertions at `ui-library.test.ts:688-711` cover two siblings, untouched colliding/unplaced siblings, immutability and no extra reads. |
| Escape while move/resize is active | `drag` (:185-214) installs one release owner; window Escape and handle Escape both reach `cancel` (:159-161), which removes producers before clearing preview | Further events from that gesture cannot recreate draft or commit. `stop` removes target move/up/cancel listeners and window Escape listener and releases held capture. A later gesture starts normally. Focused Pointer edit cancellation (:714-750) covers focused/unfocused move, unfocused corner, post-Escape move/up, successful fresh resize and pointercancel; historical real-browser capture proof supplements the DOM environment. |
| Browser pointercancel | Shared `end` (:205-208) first stops producers, then drops draft without callback | Previously completed retained edits survive. Dedicated pointercancel assertion retains the last successful h=7 layout and callback count. |
| New authored document during an edit | Parsed-document pre-effect (:102-109) invokes shared cancel and clears retained overrides; panels reload only if content identity changes | Old pointer producers are stopped before the replacement document is rendered. Even a layout-only document is authoritative for local edits while data loads survive. This cleanup is source-traced; there is no dedicated supplied active-drag host-reset assertion. |
| Unmount / new MCP result / page replacement | Renderer cleanup (:110) releases current gesture; load effect cleanup (:83-89) aborts controllers; `mountView.destroy` and App tool-result handler own unmount/remount | Old mount has no live gesture producers; old reads cannot settle into a new mount. Local edits intentionally disappear. Svelte's installed `dom/elements/bindings/size.js::bind_element_size` unsubscribes ResizeObserver listeners on effect teardown, so measurements do not acquire a separate unmanaged lifetime. No persistence recovery is required. |
| One read rejects or upstream becomes unavailable | `start` (:222-240) creates independent panel controllers; rejection becomes that panel's failed result; `settle` (:244-248) accepts only current controllers | Siblings remain usable, including layout controls. Failure display changes intrinsic height through normal observation. No automatic retry amplifies the outage. Later content/loader changes start a fresh generation. TASK-003 regression assertions cover rejection and sibling isolation; this task does not change the request policy. |
| Superseded read settles late | Load effect cleanup → `abortAll` (:217-220); `settle` checks aborted signal and current-controller membership | Late completion cannot overwrite current panel data or induce geometry from old results. Existing Dependent loads test (:343-367) asserts abort and stale-result suppression. |
| Layout-only completion or host feedback | `content` (:72-75) excludes layout; loader derives from stable `load ?? executor` identity (:61-70) | Retained panel data/in-flight reads survive layout edits and echo. Focused Layout editing proves no reload. The example's remote-loader expression introduces no new layout transport or persistence workflow. |
| Loading settles, error/notice content changes, or width changes | Svelte offsetHeight binding (:295) updates `heights`; placements (:117-122) recompute through `rowsFor`; automatic panels keep intrinsic size with align-self:start (:129-135); desktop rows stay 40px (:351-355) | Occupancy can grow/shrink without changing authored Layout or stretching row pitch. Fixed panels ignore measured heights and scroll their overflow; automatic occupancy is not capped at the authored 48-row maximum. R1 browser measurements cover loading→loaded, width changes, short header/noncollision and tall Chart/collision. Pure placement and bound-measurement tests supplement physical measurements. |
| Narrow view / return to desktop | Width derives narrow/editable (:91-94); ordered placements (:124-127), full-width styles (:129-135), and auto stacked rows (:357-358) | Narrow presentation retains authored fixed h and intrinsic auto height, hides editing controls and adds no loads. Desktop placement returns through the same geometry owner. Focused Narrow views proves ordering, width/height styling and absent controls; historical browser evidence checks physical stacking/overflow. |
| Browser/process reload | No layout storage write exists in renderer or hosts | Reload reconstructs the written document. Loss of per-render edits is explicitly required, including MCP. Server state and unrelated capabilities are unaffected. |

## Prior ledger closure

| Stable finding | Candidate evidence | Closure |
|---|---|---|
| FIND-027-1: whole-document callback drops earlier sibling completions | `WyrdView.svelte:150-155` constructs `done` from every retained completion and overlays only these authored entries; unedited panel representations are preserved. Rendering consumes the same retained entries (:112-114). | **Closed.** Two-panel non-echo callback and subsequent echo regression directly cover the prior failure. No collision/measured geometry leaks into untouched siblings. |
| FIND-027-2: Escape clears preview but leaves pointer producers active | `cancel`, release owner, common stop/end and window Escape at :158-214; parsed reset and unmount at :102-110 | **Closed.** Both movement/resize and focus states use the renderer-owned lifecycle; post-cancel move/up is inert, and a fresh gesture still completes once. |
| FIND-027-3: intrinsic panels stretch fixed grid pitch | `layout.ts:22,33-41` uses whole-row measured occupancy; `WyrdView:117-142,295,350-358` shares that placement with editing/rendering and fixed desktop rows while retaining intrinsic box height | **Closed.** Source addresses the producer rather than adding per-consumer offsets. Supplied browser evidence distinguishes a short noncolliding header at y4=208px from a genuinely colliding tall Chart pushed to row 6; settling/width evidence covers ongoing measurement. |

No new material finding proposed. No recommendation broadens cancellation into a server/process failure, introduces durable layout state, or adds optional recovery machinery.

## Verification assessment and limits

Read `verification.md`: root freshly ran nine exact focused selectors, UI and example workspace checks/builds, generated-contract check and diff check at this candidate; every command exited zero. Read actual test assertions rather than treating green status as acceptance. R1's recorded physical geometry/capture measurements and original human gallery acceptance are historical evidence, explicitly distinguished from fresh component/build results. The measured-occupancy component test stubs offsetHeight and cannot independently prove CSS geometry; the supplied real-browser measurements provide that complementary proof.

This reviewer performed source/lifecycle tracing and inspected installed Svelte observer teardown; did not rerun tests/builds or execute a fresh browser, Linux bundle, live MCP or Postgres journey. Active-drag document replacement/unmount behavior is source-supported rather than directly asserted by the supplied focused tests. No source, test, fixture, dependency or deployment change was made; only this report was written.

Skipped repeated execution and broader infrastructure checks. Risk: physical browser and platform parity beyond the recorded macOS/browser evidence remain unverified by this review.
