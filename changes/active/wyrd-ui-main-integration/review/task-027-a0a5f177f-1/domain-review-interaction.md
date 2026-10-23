# TASK-027 cumulative R2 interaction domain review

Result: **PASS**. No material proposed finding remains in the reviewed interaction, accessibility or geometry boundary.

## Immutable subject and authority

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; original base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; candidate `a0a5f177f23960822222b851716460dec02804b3`. HEAD was checked and remains this candidate. This report is the only file written by this reviewer. Other review artifacts are concurrent work, not source changes or inputs to this independent discovery pass.

Reviewed the complete cumulative TASK-027 implementation from the original base, then used R2 changes to locate remediated owners. Read original `tasks/TASK-027-view-layout.md`, both historical verdicts and validated ledgers, R1/R2 tasks and recorded evidence, and this attempt's shared subject and verification. TASK-003 foundation corrections and unrelated merged Rust/deployment changes are distinguished from layout work. No current peer report or intended verdict was consulted.

Authority applied: supplied AGENTS.md; architecture/agent-rules.md; reference router; spec-driven-development, maintainer-style, TypeScript and testing-workflows references; applicable Wyrd design/client doctrine and wyrd-doctrine.mdx; approved specification revision 12 REQ-030/036 and AC-015; Wyrd UI skill, Svelte patterns and testing/verification guidance; brand/DESIGN.md. Product and repository authority precede task mechanics and tests. The reviewed behavior is renderer-owned transient presentation, not server-owned durable state. No `.codegraph/` exists.

## Boundary and source coverage

UI-relative paths below start at `crates/wyrd/wyrd-server/wyrd-ui`.

| Boundary | Source inspected and producer-to-consumer coverage |
|---|---|
| Accepted document and identities | `packages/ui/src/lib/schemas.ts:180-226`: strict bounded LayoutSchema, optional PanelSchema.layout, unrestricted nonempty unique IDs; generated catalog projection and contract assertions. |
| Grid geometry | Complete `packages/ui/src/lib/layout.ts`: fixed constants, measured rowsFor, document-order place/collision push-down, edit bounds and rowsHeight. No duplicate geometry owner or framework was added. |
| Input, retained edits and preview | Complete `packages/ui/src/lib/WyrdView.svelte`, particularly `56-173`: parsed input, layout-stripped content key, stable load source, identity-based document reset, kept/draft/height records, placement Map, narrow ordering, styles/current, commit/cancel. |
| Interaction lifetime | `WyrdView.svelte:175-226,308-333`: keyboard step/Enter/Escape, grip blur, shared grip/corner pointerdown, captured move/up/cancel, window Escape, release/stop, capture removal and completion. Pre-effects at `105-116` cover new document and availability loss; teardown at `112` releases the gesture. |
| Accessibility and containment | `WyrdView.svelte:286-422`: semantic named button, keyboard equivalents, native focus order, visible token-based focus, touch-action on grips/corners, editing outline, fixed grid tracks, min-width/min-height, internal overflow, narrow full-width flow. |
| Component measurement | `components/Chart.svelte`: native plot box dimensions drive CSS-pixel geometry, flex fill/minimum plot size, overflow confined by the renderer; CardHeader and error/loading content remain inside the same fill container. Native offsetHeight function binding observes panel occupancy independently of chart measurement. |
| Every production host | `packages/ui/src/lib/view.ts` validates/forwards options and exposes destroy; direct component export; gallery `src/routes/_gallery/+page.svelte` echoes callbacks; `examples/ui-app/src/panels.ts` and `/routes/view/+page.svelte` use the shared renderer with remoteLoader and echo; `packages/ui/src/app/main.ts` destroys the previous view and mounts a non-echo per-render callback. All use the same renderer and geometry owners. |
| Existing proof and test support | Complete TASK-027 contract, placement, editing, cancellation and narrow test bodies in `src/lib/ui-library.test.ts:598-940`; related no-reload/host tests; `vitest-setup.ts` observer stub. Read original and both remediation evidence against the actual assertions. |

All relevant layout state writers were traced. Keyboard arrows and captured pointermove write draft. Commit alone overlays a completed layout into kept; cancel clears only draft. A new document clears kept and draft. Measurement setter replaces heights with own computed properties. Neither rendering placement nor measured occupancy writes an authored layout. Host callbacks cannot be assumed to echo, and all completion behavior was assessed for that supported case.

## Cumulative acceptance and prior-finding closure

| Obligation / prior finding | Current implementation evidence | Proof assessment | Result |
|---|---|---|---|
| Strict bounded whole-unit layout; no width alias or catalog component size input | LayoutSchema and strict panel owner; only view catalog schema gains layout | Contract selector and source/catalog assertions; historical codegen evidence remains distinct | PASS |
| Fixed 12-column, 40px rows and render-only collision push-down; unplaced panels retain content height | `layout.ts:22-40`; fixed CSS tracks `WyrdView:361-376`; boxStyle `138-145`; measured native binding `306` | Placement test covers mixed/unplaced/positive/changing height; original/R1 measured-browser geometry evidence establishes physical pitch and occupancy | PASS; FIND-027-3 remains closed |
| Whole-document completion retains every earlier sibling edit without host feedback | `commit:160-167` builds done from all kept and overlays only actual own entries | Non-echo a/b assertions preserve untouched auto/colliding siblings; feedback does not reload; retained sibling checked during pointer terminal tests | PASS; FIND-027-1 remains closed |
| Keyboard move/resize, bounded preview, completion and Escape | `current`, shared pure edit, `key:178-189`, blur and named focusable grip | Existing editing test checks Shift/arrow bounds, Enter once, Escape restoring prior completed state, input immutability and stable reads | PASS |
| Pointer Escape and pointercancel terminate both preview and producer | `cancel:170-173`, shared stop/end/window Escape `196-226` | Focused/unfocused grip/corner Escape, subsequent movement/up inert, pointercancel silent and fresh gesture works; prior real-browser proof supplements jsdom | PASS; original FIND-027-2 cause remains closed |
| Completion ends pointer production before early returns or publication | `commit:161` invokes release first; stop clears release/listeners/capture before pointerup calls commit | New non-echo blur/Enter checks for grip and corner, before-first-move completion, later inert move/up and fresh gesture; supplied trusted Chromium Tab/Enter sequences corroborate native event delivery | PASS; revised FIND-027-2 completion branch closed |
| Narrowing or losing handler cancels before controls disappear; retained edits survive | `editable:94`, pre-update cancel `114-116`; cancel leaves kept; reset observes given document identity `102-111` | Focused/unfocused move/corner width transitions, handler removal/reinstatement and no callback/outline; supplied trusted Chromium width/capture sequence confirms real removal behavior | PASS; revised FIND-027-2 availability branch closed |
| New document/reset and unmount end active producers without coupling layout to loads | Document pre-effect calls cancel and clears kept; teardown invokes release; load lifecycle remains separately keyed on content/source | Source directly covers both cleanup entries; feedback/reset assertions and mount destroy proof support lifecycle. No fresh native held-gesture-unmount probe is claimed | PASS |
| Every valid ID preserves authored/retained layout and continuously reactive occupancy | Own-entry reads at `119,129,166`; raw records `96,125`; computed spread writers `163,306`; placements Map | Constructor/toString/__proto__ authored/edit/cancel/output validation, untouched layout/absence, positive then changed __proto__ occupancy; supplied Chromium content-height change corroborates measurement mechanism | PASS; FIND-027-4 closed including writer, not merely read guards |
| Narrow views stack in `(y,x)` order, retain explicit height and disable editing; overflow stays internal | `ordered:134-136`, boxStyle, narrow grid and fill overflow CSS | Exact narrow test and supplied original mobile browser evidence (full width, no grips, contained scrollWidth); no new independent mobile screenshot claimed | PASS |
| Charts fill resized panels and redraw; light/dark all-host behavior; MCP state limited to current render | Shared fill and Chart dimension bindings; mode tokens; actual gallery/example/AppBridge wiring above | Original gallery/AppBridge desktop/light/dark/mobile and redraw evidence, human acceptance 2026-10-20, current UI/example checks/builds. Source changes do not require replacing that approval | PASS within supplied evidence |
| No persistence, add/remove/collapse, component size option or new dependency | Complete attributed cumulative diff and shared owner/caller trace | Static diff/source evidence | PASS |

## Event and accessibility assessment

The terminal correction sits at the actual shared producer owner. `commit()` releases even when no draft was produced, preventing later movement from reviving a just-completed gesture. Pointerup calls stop before commit, so cleanup is safe when already complete. Escape and pointercancel clear only the unreported preview. Availability cancellation uses the existing pre-update facility: preview and listeners are gone before grip removal can trigger blur. Restoring editability retains earlier completed edits because callback changes no longer masquerade as a new document. A genuinely new document still resets edits. Both move and resize use this same lifetime, including non-echo MCP/mount hosts.

The grip is a native button with a meaningful accessible name and a visible focus ring. Arrow/Shift+arrow give the corner's keyboard equivalent on that button; Enter and blur complete, Escape cancels. The decorative pointer corner need not become a separate keyboard stop to satisfy the approved equivalent interaction. Focus removal on narrowing/handler loss is covered by cancellation rather than an invented focus manager. Internal fill scrolling keeps oversized chart/content within its own panel. No material accessibility or responsive regression was established from the reviewed source and supplied browser evidence.

The arbitrary-ID repair is complete at the record owner. `Object.hasOwn` distinguishes authored fallback from actual retained/measured data, including zero; computed-property immutable writes safely establish own __proto__ entries. Native readonly dimension function binding retains Svelte observation and teardown instead of adding an observer abstraction. Positive then changed measured-height tests reject the previous deep-proxy writer failure. Transient heights do not leak into callbacks.

## Verification credibility and limits

This review independently inspected source, cumulative diff, authority, tests, prior ledgers and recorded results. It did not rerun package builds or browser automation; the orchestrator owns sequential current execution. This attempt's verification records nine exact selectors (including all five layout scenarios), UI check/build, example workspace check/build and clean source identity. R2's full ten-test-file result is supplied historical execution, not a fresh rerun by this reviewer.

The supplied R2 browser proof uses installed Chromium with trusted mouse/keyboard against a non-echo mount, not dispatchEvent-only synthetic capture. Callback counts, unchanged post-terminal row positions, retained sibling output and disappearance/reappearance of grips directly address the prior native-focus/capture failures. The Chrome extension's inability to reach the origin does not invalidate the usable Chromium evidence. It also does not prove every browser or deployment platform.

`onlayout` removal is proved through the existing component rerender facility in jsdom. Static mountView intentionally has no update API. The source uses the same pre-update availability cancellation observed for the physically exercised breakpoint transition, and the component tests separately assert callback removal/restoration preserves kept state and emits nothing. Approved authority and R2 do not require adding an update API, new harness or new dependency for that branch; this is a documented verification limit, not an unimplemented contract. Normal new-document/unmount cleanup is source-backed and existing mount lifecycle proof, without claiming a new native held-capture teardown run.

ResizeObserver.notify extends the existing jsdom stub and delivers callbacks only for each observer's current target set. Native binding unobserve removes targets; disconnect clears targets. The live observer collection retains observer objects but empty observers produce no target events, so inspected test teardown cannot notify an unmounted element. This test-only mechanism is sufficient to drive the dimension bindings without pretending jsdom computes geometry; real Chromium evidence supplies physical geometry/capture corroboration. No production observer or harness was introduced and no prior assertion was weakened.

## Material proposals and disposition

Material proposed findings: **none**. No corrective API, new infrastructure, host-specific guard, persistence decision or speculative lost-capture framework is needed for the accepted scope. FIND-027-1, FIND-027-2 (original and revised terminal branches), FIND-027-3 and FIND-027-4 are closed by current source with mapped proof.

Overall domain result: **PASS**.

Skipped fresh browser execution, package reruns, live backend/MCP parity and Linux/image qualification. Risk: browser/platform coverage remains the documented supplied scope; no demonstrated interaction, accessibility or geometry defect remains in this candidate.
