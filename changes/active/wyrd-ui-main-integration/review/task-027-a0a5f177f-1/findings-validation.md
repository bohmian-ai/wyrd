# TASK-027 cumulative R2 independent findings validation

The deduplicated active ledger contains **one new finding, FIND-027-5**. I027-R3-01 is confirmed as a reachable reset regression; its correction is narrowed below to the existing authored-document reset owner. FIND-027-1 through FIND-027-4 remain closed for their diagnosed causes. This report supplies validation, not the orchestrator's task verdict.

## Immutable subject and coverage

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; original base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; candidate and independently inspected HEAD `a0a5f177f23960822222b851716460dec02804b3`.

Read this attempt's subject.md, verification.md, claims-comparison.md, both task implementation reports, standards-review.md, maintainer-review.md, system-review.md, domain-review-interaction.md and followup-review.md. Read the two historical validated ledgers/verdicts, R1/R2 remediation tasks and appended execution evidence. Applied approved specification revision 12 REQ-030/036 and AC-015, original TASK-027, supplied AGENTS.md, agent rules, reference router, spec-driven-development, maintainer style, applicable design/client and doctrine public-surface boundaries, TypeScript and Wyrd UI/Svelte/testing guidance. No `.codegraph/` exists.

Inspected the cumulative task-owned diff and complete current schema, layout, renderer and mount owners, their tests, production hosts and the R2 observer stub. R2's delta locates changed owners but does not substitute for cumulative acceptance. Separately owned TASK-003 foundation corrections and unrelated upstream changes are not TASK-027 findings. The only file written by this validator is this report; no source, fixture, harness or dependency changed.

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui`, unless qualified otherwise.

## Authority, actual input and reachability

REQ-030 positively includes user-authored Svelte apps and publishes the renderer at `wyrd-ai-ui/view`. `packages/ui/src/lib/view.ts:13` exports WyrdView and the package's `./view` export actually exposes it. It is therefore insufficient to audit only the currently shipped gallery, example and static mount helper. REQ-036 makes the authored document the layout authority, requires valid layouts to render and whole-document callbacks, and leaves retention to the host. R1 explicitly preserves resetting local state on new authored input; R2 preserves document cleanup while distinguishing handler availability from document changes.

Normal host-owned deeply reactive Svelte state reaches this public component. The repository's Svelte guide uses mutable `$state` and derived props; its anti-pattern against the child mutating props does not prohibit the host changing its own state. More decisively, the actual renderer at `WyrdView.svelte:56,73-89` already consumes nested reactive document changes for validation and loading. R2 changed only the local editor reset from `parsed` to top-level `doc` identity. Consequently this is an inconsistency between existing reactive consumers of the same accepted input, not a proposed new update API or a requirement inferred solely from silence about immutability.

The concrete failing input needs no exotic value: a schema-valid document, two unique ordinary IDs, valid Chart `{type:'stat', sql:'select 1'}` inputs, bounded layouts, and valid `{last:'6h'}` range. `PanelSchema.input` is intentionally `unknown` and validated per component at load; successful view validation does **not** imply arbitrary JavaScript input is JSON-serializable. The finding and selected proof use ordinary valid component inputs. No schema restriction, general object-diff protocol, cycle support, reserved-ID rule or new handling of arbitrary invalid component inputs is required. Existing `content` already serializes range and panel inputs, so unrelated pre-existing serializer limits are not promoted into this finding.

Gallery and example use `$state.raw` replacement; AppBridge destroys/remounts on tool results; mountView exposes only destroy. These inspected callers avoid the defect. They do not remove the expressly shipped direct-Svelte component from approved custom-app scope.

## Complete shared owner trace

| Authority | Producers/writers | Consumers and lifetime |
|---|---|---|
| Accepted authored document | Host `doc`; strict ViewDocumentSchema with LayoutSchema (`schemas.ts:183-226`); renderer safeParse at `:56` | validateView, generated catalog, parsed rendering and panel-generation input. WyrdView never writes host input. |
| Reset | `given = $derived(doc)` and sole clearing pre-effect (`WyrdView:102-111`) | Calls cancel and clears kept. Tracks replacement identity, omitting same-reference nested state changes. Teardown separately releases active pointer ownership. |
| Retained authored edits | Initialization/reset `{}` and commit's computed-property replacement (`:96,109,163-165`) | Own-entry layouts selection (`:118-120`) and full callback overlay (`:166`). No host merge or second store owns this state. |
| Preview/held producer | key arrows (`:182`) and shared drag move (`:205-206`) write draft; commit, cancel and pointercancel clear it | layouts, placements, current, styles and outline. Grip and corner share drag; its `from` is fixed at pointerdown. stop removes target move/up/cancel, window Escape and capture (`:208-225`). |
| Edit availability | Measured width/narrow and callback presence (`:91-94`) | Independent pre-effect cancels before control removal (`:114-116`), deliberately preserving kept. It cannot repair authored-document reset. |
| Measurements/geometry | Native offsetHeight binding replaces raw own-key height record (`:125,306`); pure rowsFor/place | One placements Map feeds styles, edit origins and narrow ordering. Heights/collision displacement remain transient and are absent from callbacks. |
| Panel generation | Layout-stripped stable `content`, stable loader source, brushed range (`:61-89`) | start/abortAll/settle (`:228-260`) isolate data cancellation and stale completions. Actual input/range changes restart reads; layout changes must not. act (`:263-284`) retains navigation/brush ownership. |

Read the full bodies of commit, cancel, key, drag and its closures, reset/availability/load effects, boxStyle/current, start/abortAll/settle/act, layout place/edit, mountView/remoteLoader, and every production host before selecting the correction. Chart's range brush is a different capability, not an existing document-reset owner to replace or duplicate.

## Proposal dispositions and historical closure

| Claim | Independent disposition |
|---|---|
| I027-R3-01; focused follow-up's reset diagnosis | **CONFIRMED**, one new FIND-027-5. A valid nested host write updates parsed authority while reset is skipped. Display, callback and held-draft effects share this cause. |
| Follow-up's full validated value signature plus replacement identity | **CONFIRMED with bounded interpretation:** reuse the existing stable serialization pattern on the parsed authored document; retain reference observation and the separate layout-stripped load key. Do not treat view validation as a new promise about arbitrary JavaScript `unknown` input serialization. |
| Behavior/system/maintainer/standards/domain general reset PASS statements | **REVISED in scope.** They prove new-reference replacement, current shipped hosts and unrelated handler changes. They do not cover direct Svelte nested authored writes. Their other source-backed closure remains applicable. |
| Previously rejected inline remoteLoader recreation, cwd observation, inferred annotations/style | **REJECTED as findings**, as before: no current source-backed consequence or task-required correction. No optional advice or new ledger entry. |

| Prior ID | Current source and proof | Closure |
|---|---|---|
| FIND-027-1 | commit builds done from every retained completion and overlays all own entries (`:163-166`); editing test `:731-755` checks non-echo A then B, untouched auto/collision panels, immutable input, feedback/no reload | **CLOSED** for the original omitted-sibling serializer cause. FIND-027-5 supplies stale retained state to this correct serializer; it does not rediscover missing overlay logic. |
| FIND-027-2, including R2 terminal additions | release runs before commit's early return (`:161`); cancel reaches it (`:170-173`); stop removes the actual producers; availability pre-effect precedes control removal. Pointer tests `:789-920` cover Escape, pointercancel, held blur/Enter, no-move completion, later inert events, fresh gestures, narrowing and callback removal/restoration | **CLOSED** for original Escape and R2 completion/availability causes. New-reference reset and teardown still release correctly. FIND-027-5 is the newly introduced failure to observe a document transition, not missing cleanup inside those owners. |
| FIND-027-3 | rowsFor/place reserve measured occupancy; desktop rows stay fixed; auto boxes align start; current uses that Map (`layout.ts:22-52`, renderer `:125-150,306,361-376`). Grid test covers short/tall/zero/large and changing height | **CLOSED** with supplied historical physical R1 geometry/settling/width evidence distinguished from jsdom styles. No change to fixed pitch or input/callback geometry semantics. |
| FIND-027-4 | Own-entry reads (`:119,129,166`), raw height state and computed-property immutable native-binding writes (`:125,306`), existing retained computed-property writes (`:163`) | **CLOSED**, including writer/reactivity. Current tests compare constructor/toString/__proto__ placement, valid unchanged/edited callbacks and positive then changed __proto__ occupancy (294→400). Read guards alone would not pass that proof. |

All remaining cumulative obligations have current source and mapped evidence: strict bounded optional layout/obsolete width rejection; source-driven view-only catalog publication; render-only collision resolution; component fill/overflow; optional accessible keyboard/pointer editing; narrow ordered fixed-height stacking; host wiring/render-local MCP state; no persistence, sizing inputs, add/remove/collapse or dependency. None justifies a separate finding. The native observer binding owns observation/teardown; the test stub's target notification follows actual Svelte consumers, and unobserve/disconnect clear targets. No production observer framework or speculative stub expansion is required.

## FIND-027-5 — CONFIRMED / REGRESSION

**Discovery sources:** I027-R3-01 and focused followup-review. **Violated obligations:** REQ-030 custom Svelte host/public renderer, REQ-036 authored layout authority and whole-document completion, original scenarios 2–3, and preserved new-authored-input reset. **Exact correction location:** `packages/ui/src/lib/WyrdView.svelte:102-111`; affected consumers `:118-120,160-173,196-225`. Validation/content siblings at `:56,73-89` establish the split authority.

**Reproduction and consequence.** Direct Svelte host passes a valid reactive document with A at y0 and a non-echo handler. Complete A at y1, so kept.a stores y1. Host writes its own `doc.panels[0].layout.y = 7` through the same proxy. safeParse now accepts y7, but `given` remains the identical proxy and reset never calls cancel/clears kept. Own kept A wins over authored A in layouts; the screen remains y1. Completing B causes the otherwise correct whole-document serializer to overwrite current authored A y7 with stale kept A y1. A host accepting that output loses its authored update.

Similarly, an accepted reactive range or panel-input change updates the load key but not given. A held draft and its fixed-origin move/up closures survive. Later move can recreate the old draft and release can publish it into the new authored document. This is one missing reset dependency at the source, not separate display/callback/pointer bugs. Existing availability cancellation preserves kept by design; loader cancellation owns reads, not gestures.

**Independent read-only proof.** Ran `mise exec -- node --conditions=browser --input-type=module` in the UI workspace with installed Svelte compiler/runtime and actual candidate `packages/ui/dist/schemas.js`. Compilation of actual WyrdView yielded zero warnings and emitted `given = $.derived(() => $$props.doc)`. Native proxy/derived/effect/flush probes using the exact schema/reset/selection/serializer expressions independently asserted accepted authored y7, effective y1 and callback A y1. A second probe validated the Chart input too, then observed layout-only change and range change:

```json
{"compilerWarnings":0,"validChart":true,"resets":1,"releases":0,"loadChanges":2,"keyChanges":3,"authoredY":7,"keptY":1,"heldDraft":true}
```

Here reset count 1 is initialization; the full-value key observes initial, layout and range states, whereas the existing content key observes only initial and range. A separate assertion establishes that replacement with an equal serialized new object still runs the reference-dependent observer. The first probe's optional replacement check attempted structuredClone on schema output containing a proxy and failed with DataCloneError; rerun used JSON cloning for this JSON fixture and passed. No application test failed or was changed. These are installed-runtime state probes and complete source traces, not claims of mounted DOM or trusted-pointer browser reproduction.

**Selected smallest complete correction.** Extend the existing document-reset pre-effect to observe both actual replacement identity and a stable full validated authored-document value signature, including layout, title, range, panels and inputs. Reuse parsed and the existing JSON.stringify derived-value comparison pattern; an unsuccessful parse can use the existing undefined invalid sentinel. It must reach the existing untracked cancel/kept clear before rendering accepted changed content. No new validator, watcher/store, lifecycle owner, host merge/update protocol or dependency is earned.

Retain reference observation: equal-value new objects still represent the existing explicit replacement/reset boundary. Retain stable value equality: reparsing/handler-only rerenders of unchanged documents must keep completed edits, closing the actual R2 regression without reopening it. Keep the loading content key separately layout-stripped, so authored layout-only changes reset editing state without new reads or brushed-range reset. Preserve input immutability, native measurements and own-key records, all existing completion/cancellation/availability behavior and server/data ownership. Resetting on parsed-result allocation alone reopens R2; watching only layouts misses accepted range/input transitions; using only content misses authored layout writes; downstream callback guards cannot restore correct display or terminate the producer. The existing renderer owns all three affected transient states and is the correct correction boundary.

**Focused closure proof.** Extend existing component tests using an actual native Svelte reactive document proxy and valid component inputs; private fixture structure remains implementation-owned:

1. Complete A without echo; mutate its authored layout through the same proxy to y7; flush and assert rendered authored placement. Complete B and assert the whole callback preserves A y7. Layout-only mutation adds no reads; input remains host-owned and unchanged by editing.
2. With a held move and held corner preview, change accepted range or panel input through the same proxy. Assert reset removes preview and old move/up cannot alter placement or emit; a fresh gesture still works. Source shares one reset/drag owner, so no new native browser infrastructure is needed to prove this reactive dependency.
3. Preserve existing same-document handler removal/restoration retaining completed edits, unchanged-document rerender, and genuine new-reference reset (including equal-content replacement). Keep prior terminal/own-ID/geometry regression assertions intact.

Run these exact affected selectors, then UI check/build and diff check:

```sh
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Pointer edit cancellation$'
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- git diff --check
```

No new product, API, durable ownership, persistence or concurrency-semantics decision is needed. Do not require a backend journey, codegen, repository aggregate, platform release or new harness for this renderer/test-only correction.

## Evidence limits

Root's current nine exact selectors, UI/example checks/builds and native Node smoke are green; their test bodies do not cover same-reference authored updates. Original/R1/R2 browser observations and human approval remain supplied historical evidence. I ran only the described read-only runtime/compiler probes and identity/source inspection, avoiding duplicate builds. HEAD remained the immutable candidate and tracked source was clean.

Skipped fresh DOM/browser capture, duplicate package suites, live backend/MCP parity and Linux qualification. Risk: the validated direct-Svelte reset defect remains until corrected and proved by the focused component assertions; historical browser proof does not close this newly exposed reactive path.
