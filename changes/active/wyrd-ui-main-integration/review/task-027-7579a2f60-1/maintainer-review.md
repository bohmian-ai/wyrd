# TASK-027 maintainer review

**Result: PASS.** No material maintainer findings are proposed.

## Subject and authority

Reviewed immutable cumulative `5e57416d562386066c1aadda9dd01783b9cd4eb5...7579a2f603c023daae0d15b9d5cc3013eec6f00d`, including the latest reactive document reset. HEAD remained the candidate when checked. Source and tests were not modified. This is an independent maintainer review; no current-round peer report was read.

Read the shared subject, original TASK-027, specification revision 13 REQ-030/036 and AC-015, root AGENTS.md, agent_docs/index.md and maintainer-style.md, architecture/README.md and wyrd-design.md client-model section, changes/spec-driven-development.md, TESTING.md, the Wyrd UI skill, its Svelte and verification references, and brand/DESIGN.md. The subject establishes no nested AGENTS.md in the changed UI/example paths and no root CodeGraph index. Historical FIND-027-1 through FIND-027-5 were treated as hypotheses rather than acceptance authority. TASK-003 changes were inspected at layout seams; their independent behavior and unrelated PR #205 content are outside this task's maintainer verdict.

## Changed-surface coverage

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui/` unless stated otherwise.

| Changed surface | Source, callers and proof inspected | Maintainer assessment |
|---|---|---|
| Layout contract and publication | `packages/ui/src/lib/schemas.ts::LayoutSchema`, `PanelSchema`, `ViewDocumentSchema`; `packages/ui/scripts/catalog.ts`; generated `catalog.json`; `view.ts::validateView`; layout document contract test | PASS. Layout extends the existing strict document owner. Component inputs remain size agnostic. Generator continues to consume the schema owner; no second hand-maintained layout schema or compatibility field is introduced. |
| Pure geometry | Full `packages/ui/src/lib/layout.ts`; renderer `layouts`, `placements`, `ordered`, `boxStyle`, `current`; grid-placement assertions | PASS. Small pure module owns collision and snapping rules, while state remains with the renderer. It accepts positional heights and returns placements without rewriting documents. `rowsFor` and `rowsHeight` make measured occupancy and fixed row pitch explicit. |
| Local completed/draft state | Full `WyrdView.svelte`; `commit`, `cancel`, `given`, `authored`; non-echo, feedback, unchanged-reference, equal-content replacement and reactive-host tests | PASS. Retained edits, in-progress edits and authored input are distinct, documented values. Commit overlays all completed own entries into one whole-document callback. The R3 correction extends the existing reset dependency rather than adding a second reset/merge workflow. |
| Pointer and keyboard lifecycle | `key`, `drag`, `release`, `commit`, `cancel`, editable pre-effect and teardown; pointer cancellation test through all terminal paths | PASS. Keyboard and pointer use the same `edit` rule and completion owner. Stop removes the actual event producers before completion or cancellation, including no-draft completion. The lifecycle is local and discoverable beside its controls. |
| Arbitrary panel IDs and measurement | `kept`/`heights` reads and computed-property writes, `$state.raw`, Map placement owner, height binding; prototype-name and changing-height tests | PASS. Own-entry access and whole-object writes preserve the accepted string-key contract without wrappers or new identity restrictions. Tests cover both readers and the reactive writer. |
| Rendering and resize consumers | Full renderer markup/styles; Chart measurement/fill CSS and cumulative Chart diff; CardHeader and error rendering seam | PASS. Arrangement remains with WyrdView. Components receive existing props and available dimensions; overflow containment remains a renderer concern. TASK-003 plot/label changes introduce no duplicate layout owner or size parameter. |
| Public mounting and MCP render | Full `view.ts`, `packages/ui/src/app/main.ts`; `MountViewOptions.onlayout`, direct WyrdView export and per-tool-result mount/destroy | PASS. One typed optional callback extends the existing host boundary. MCP opts in and relies on existing renderer state; destroy/replacement bounds retention without persistence machinery. |
| Gallery and example callers | Gallery document and onlayout assignment; `examples/ui-app/src/panels.ts`, `examples/ui-app/src/routes/view/+page.svelte`; fixture changes | PASS. Existing view documents gain placements, and host-local assignment retains edits for the current visit. No new state store, save contract or framework dependency is introduced. |
| Tests and fixtures | Four original TASK-027 scenarios, pointer terminal cases, R3 reactive-host additions; `src/lib/ui-fixtures.svelte.ts::reactive`; ResizeObserver setup | PASS. Assertions observe placement, immutable input, complete callback documents, load counts and inert stale gestures. Native Svelte proxy fixture is five lines and directly exercises the public host contract. Existing component harness is reused. |
| Documentation and declarations | Public callback/type comments in schema, renderer and mount options; original and remediation evidence, generated catalog diff | PASS. Comments explain ownership and lifetime rather than narrating implementation. Public callback and ViewDocument types match their actual values. No Rust/Python export or authored `.pyi` change belongs to this task. |

## Existing owners and alternatives

The original renderer already owns panel arrangement and loads, so putting editing in WyrdView extends the existing workflow. The new layout module is pure caller-owned geometry, not a competing state owner. Chart retains its existing measured box and the view supplies containment, so there is no parallel component-sizing API. Layout publication still follows schemas → catalog generator → catalog consumers. The one new reactive fixture uses installed Svelte runes and the current testing harness; no additional framework or dependency is necessary.

`content` deliberately excludes layout to protect panel loads; `authored` includes it to observe the host's current document. Those keys serve different real consumers. Keeping both is justified: using the load key for reset would miss authored layout changes, while using the full authored key for loading would reload on layout edits. `given` additionally preserves replacement-by-reference semantics for equal-content new documents. The reset effect cancels the gesture and clears retained edits together, at their existing owner.

Inspected the example's inline `remoteLoader('/view/load')` using the installed Svelte compiler, read-only. Compilation memoizes that expression in a derived value with no dependency on the host document. An onlayout assignment therefore does not recreate its loader merely because the document changed. This checked a real caller seam without proposing a speculative hoist or wrapper.

## Prior-finding closure from source

| Prior finding | Maintainer source evidence | Assessment |
|---|---|---|
| FIND-027-1, cumulative completed edits | `commit` spreads kept entries and maps every own completion onto parsed panels; tests finish A and B without host echo and preserve untouched panels | Closed; one serializer owns the complete callback. |
| FIND-027-2, gesture terminals | `release` is reached before commit's early return and by cancel; stop removes listeners; editable transition and teardown share cleanup | Closed; no residual alternate terminal workflow found. |
| FIND-027-3, content-height occupancy | `place` reserves measured whole rows; renderer preserves fixed row pitch and auto content height; `current` starts from the resolved placement | Closed; measurement and geometry are explicit rather than consumer-specific corrections. |
| FIND-027-4, arbitrary IDs | Own-entry guards, raw object state, computed-property replacement and Map lookup; constructor/toString/__proto__ tests include subsequent measurement changes | Closed; writers and readers agree on the same key contract. |
| FIND-027-5, reactive authored reset | `authored` derives serialization of accepted parsed values; reset reads both it and given, then invokes cancel and clears kept; actual native proxy tests cover nested layout/range/input changes | Closed; root dependency is fixed with the existing reset owner and its adjacent behavior preserved. |

## Findings and verification limits

Material proposed findings: **none**. Non-blocking style notes: none requiring action.

Verification evidence supplied for the candidate is green package/app check, 40/40 tests, build and 2/2 real-Postgres UI integration tests. This pass independently inspected source and test assertions and compiled the example page in memory. One preliminary compiler read used an incorrect relative path and failed with ENOENT; the corrected absolute-path invocation succeeded and produced the memoization evidence above. This was a read-only inspection failure, not an application or test failure.

Skipped duplicate integrated lanes and fresh browser captures; physical pointer capture, final geometry and theme qualification rely on the supplied historical browser evidence and candidate-integrated checks. No source-level material maintenance risk remains identified by this review.
