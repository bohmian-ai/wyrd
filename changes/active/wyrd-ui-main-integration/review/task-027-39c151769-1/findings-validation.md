# TASK-027 independent structured Ponytail validation

Validation complete. **The independently validated active finding ledger is empty.** FIND-027-1 through FIND-027-6 are closed for their diagnosed causes. No remediation or specification decision is needed.

## Immutable subject and inputs

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`; cumulative base `5e57416d562386066c1aadda9dd01783b9cd4eb5`; candidate and independently checked HEAD `39c15176955edca785cc578168c67e1efcb80ea5`. Original task: `changes/active/wyrd-ui-main-integration/tasks/TASK-027-view-layout.md`. Approved specification: `changes/active/wyrd-ui-main-integration/spec.md`, revision 13. Revision 13 preserves the original task's layout obligations and governs the flat Card reference seam.

Read all six current discovery reports: task-review-behavior.md, task-review-invariants.md, standards-review.md, maintainer-review.md, system-review.md and domain-review-interaction.md; also subject.md, claims-comparison.md and verification.md. Each discovery proposes no material finding. No follow-up report exists; the comparison records why no conflicting claim or untraced shared cause required one. This validation does not infer completion from that agreement.

Read the historical FIND-027-1..6 diagnoses and remediation evidence, including TASK-027-R4. Independently inspected the actual cumulative source diff with `git diff 5e57416d5...39c151769`, current source and callers; used the latest renderer diff only to locate the R4 join. Historical review/planning records and independent TASK-003 changes are provenance or adjacent seams, not independent TASK-027 findings. The previous rebase-preservation audit is supplied history; no new rebase archaeology is claimed.

Applicable authorities inspected include root AGENTS.md, spec-driven-development, maintainer-style and agent-docs index, agent-surfaces, architecture authority map and client model, TESTING.md, approved REQ-030/036 and AC-011/015, and UI/Svelte/testing and brand guidance. No CodeGraph index applies. No new sensitive server, persistence, security, SQL or tenancy boundary is introduced by the task-owned layout change.

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui/`, except explicit repository paths.

## Independent source and caller coverage

Read the full renderer and geometry owners, including `current`, `commit`, `cancel`, `key`, `drag` and all its event closures, authored/editability/load effects, `boxStyle`, `start`, `abortAll`, `settle` and `act`; full `layout.ts`; strict layout/view schemas; mounting and remote loading; catalog generator and generated layout projection; the complete layout regression scenarios and native reactive fixture/measurement facilities. Inspected Chart's measured sizing and containment, loader validation/range behavior, and changed executor/component seams.

The production caller search reaches the gallery, example `/view`, and mountView, used by the MCP App. The public WyrdView export also accepts direct custom Svelte hosts. All reach the same state and geometry owners. Gallery/example echo complete callback documents; mountView/MCP can retain local edits without echo. MCP destroys the previous renderer for each tool result. No caller requires a separate merge, remount protocol or layout store.

| Authority / producer | All relevant writers and sibling consumers traced | Assessment |
|---|---|---|
| Accepted authored document | Host doc -> strict safeParse -> authored signature, layout-stripped content, layouts/placements, header/errors and callback serialization | Layout authority and IO invalidation are intentionally distinct. Accepted nested host writes and replacement identity both reset local editing. |
| Retained edits and preview | key and captured pointer move write draft; commit writes kept and clears draft; cancel/reset clear preview; reset clears kept | Rendering and callback overlay read only own entries. All completions serialize the same accumulated state. |
| Measurement and geometry | Native offsetHeight function binding replaces raw heights record; own height reads plus layouts feed place; Map feeds styling, current edit origin and narrow ordering | Intrinsic extent, collision occupancy and fixed pitch share one geometry authority. Render-only displacement never becomes an untouched panel's authored layout. |
| Gesture lifetime | drag installs element pointer and window Escape listeners; stop removes listeners/capture; commit/cancel/reset/editability/unmount invoke release | The producer is stopped before completion's early return, publication or affordance removal. Later events from a disposed gesture cannot recreate its preview. |
| Read generation | content/range/loader effect -> start -> current panel rows/controllers; abortAll and settle own cancellation and accepted outcomes | Result state is independent of identity membership; refused/failed/loading/invalid-input panels remain legitimate current rows. Layout edits do not start reads. |
| Narrow presentation join | Current parsed placements plus effect-owned loaded panels -> membership filter -> stable native coordinate sort -> template | Every comparator operand belongs to the current placement Map. No old loaded-only ID is dereferenced. Wide behavior and loading ownership remain unchanged. |

## Prior finding validation

| Stable ID | Independently inspected source and focused proof | Disposition |
|---|---|---|
| FIND-027-1 — whole callback loses completed sibling edits | `WyrdView.svelte:167-174` forms done from all kept entries and the completion, then overlays every corresponding current parsed panel. It does not serialize placement displacement. Layout editing at `ui-library.test.ts:778-800` completes a then b without echo, checks both callback layouts plus untouched auto/colliding siblings, input immutability and echo without reads. | CLOSED; original serializer cause removed. |
| FIND-027-2 — gesture producer survives cancellation/completion/availability loss | `commit:167-174` releases before its early return; `cancel:177-180` releases; `drag:203-233` removes actual move/up/cancel/window listeners and capture. Authored/editability effects at 107-119 cancel; teardown releases. Pointer edit cancellation at test 867-1028 covers focused/unfocused grip/corner Escape, pointercancel, held blur/Enter, completion before movement, handler loss, narrowing and subsequent inert events/fresh gestures. | CLOSED for original Escape and revised common terminal-entry scope. |
| FIND-027-3 — intrinsic occupancy distorts fixed row pitch | `layout.ts:22-52` computes occupied rows from measured height and gap, copies layouts and resolves collisions, and shares rowsHeight pitch. Renderer height binding at 314 replaces own measurements; CSS at 369-380 uses fixed 40px tracks and narrow auto tracks. boxStyle keeps unplaced content intrinsic. Grid placement at test 665-731 checks mixed/plain, short/tall and changing measured occupancy. | CLOSED; physical geometry qualification remains historical browser evidence. |
| FIND-027-4 — accepted prototype-named IDs corrupt overrides/measurements | Renderer own-entry reads at 122 and 132, computed-property raw record writes at 170/314, own callback overlay at 173 and native Map placements cover both producers and consumers. Grid/edit tests use constructor/toString/__proto__, validate emitted documents, retain untouched entries and update positive __proto__ measurements 294 -> 400. A reads-only repair would not satisfy those assertions. | CLOSED including measurement writer/reactivity cause. |
| FIND-027-5 — reactive authored writes leave stale retained edits/held producer | given and full validated authored value at 102-114 drive the existing reset; content at 73-89 remains layout-stripped. Native-proxy tests at 834-863 show authored y7 superseding local y1 and surviving a later sibling completion, with no layout reads; tests at 999-1028 show range/input writes terminate held move/corner previews. Equal-content replacement and handler-only changes have separate controls. | CLOSED; no downstream callback-only guard substituted for reset. |
| FIND-027-6 — narrow replacement compares obsolete loaded IDs | `ordered` at 141-143 filters using placements.has before reading coordinates. The extended Narrow views case at 1030-1094 replaces two outgoing rows with c/d, reduces membership, checks abort and late-result exclusion, current loading/failure visibility, invalid recovery, same-ID reload and layout-only no-read ordering. Existing wide replacement test at 415-425 remains applicable. | CLOSED by the selected join-owner correction and persisted focused regression. |

These are separate diagnosed sources, not six labels for a remaining common failure. R4 does not depend on kept/draft or an active gesture. Reset, serialization, measurement, producer cleanup and narrow membership each retain their own valid role.

## R4 reachability, minimal boundary and preserved behavior

The prior failing state is reachable through the approved direct-Svelte renderer: parsed placements change immediately on authored replacement, while loaded rows change in the ordinary load effect. With at least two outgoing rows, sorting could dereference absent coordinates. At the current candidate, filter and comparator synchronously use the same placements authority; neither invokes IO nor changes that Map. Removed IDs cannot enter the comparator. Native stable sort operates on the newly filtered array, preserving document-order ties and leaving panels itself unmodified.

The retained consumer guard belongs here because this projection creates the cross-generation join. Each input independently remains valid. Moving loading into a pre-effect, adding generation state, freezing IDs, inventing fallback coordinates or forcing host remounts would move or duplicate adjacent ownership. Current loading, failed and panel-validation-error rows are admitted because start emits their IDs from the same parsed panel list as placements; filtering never checks props or success. Validation-error retention is established from source, rather than falsely claimed as a separately executed R4 assertion. Existing controllers continue to reject aborted or obsolete settlement, including same-ID replacements.

The persisted test exercises actual mounted Svelte scheduling with two outgoing rows; it does not avoid comparator execution through a one-row setup, rewrite the assertion, add sleeps/retries, or weaken a gate. The recorded expected TypeError RED and focused GREEN align with the production correction. This is adequate bounded closure evidence together with source tracing and supplied candidate execution; agreement among reviewers is not the proof.

## Deletion and reuse ladder / cumulative acceptance

1. Required layout, order, handler output and cancellation cannot be deleted. No persistence, add/remove UI, collapsing, per-viewer storage, sizing input, ServiceSpec extension or new dependency is present in the attributed implementation.
2. Existing owners remain extended: PanelSchema/validator/catalog generator, WyrdView state/lifecycle and mounting, Chart measurements, and existing loaders/controllers. The former CSS arrangement cannot by itself supply required document-order push-down, intrinsic occupancy or callback output; the small pure layout module supplies those missing policies without retaining state.
3. Native CSS Grid, Pointer Events/capture, Svelte derivations/effects/dimension bindings, Array filter/stable sort, Map and own-property operations cover the implementation. Chart's brush is a separate SVG time-range capability, not a reusable dashboard editing lifecycle. No replacement framework or general store is earned.
4. R4 is already the minimum shared correction. Removing its membership test recreates the diagnosed exception; moving it to hosts or result consumers fragments the invariant. No additional abstraction, installed/new dependency or test harness is necessary.

Source and persisted assertions also support the remaining original obligations: strict integer/bounded optional layout and width rejection; view-only schema publication; 12 columns/fixed pitch; mixed/unplaced collision-safe rendering without mutation; fill/overflow and chart measurement; optional pointer/keyboard editing using common edit bounds and snapping; whole-document once-per-completion output without reads; narrow full-width ordered/read-only behavior; gallery/example/MCP wiring and per-render lifetime. Historical browser/human proof remains the appropriate evidence for physical sizing, focus/capture and light/dark desktop/mobile appearance, rather than jsdom style strings alone.

## Final proposal disposition and ledger

| Input | Disposition |
|---|---|
| Both implementation reviews, standards, maintainer, system and interaction reviews | Their proposed finding union is empty. Independently source-validated as empty; no CONFIRMED or REVISED active issue. |
| Prior FIND-027-1..6 | Closed with current producer-to-consumer evidence above; no ID reopened or reassigned. |
| Non-blocking test-name convention note | No material behavioral/public-contract consequence; not an active proposal or remediation requirement. |

**Final deduplicated active ledger: empty.** No finding IDs, decision-complete corrections or remediation task are required. No unresolved authority conflict, missing correction decision or source-tracing blocker remains.

## Verification provenance and limits

Fresh validator evidence is source/diff/caller/test inspection, clean cumulative `git diff --check`, checked HEAD and worktree identity. No source/test/generated file was modified. Only this report is written. Current supplied integrated checks cover fmt/lints/codegen, TS native/unit and queue lanes; UI package/app check zero errors, 40/40 tests, build and real-server integration 2/2. Execution was not repeated or presented as independently rerun. R4's focused RED/GREEN and original browser/gallery acceptance are historical evidence, distinguished from current source proof.

HEAD remained `39c15176955edca785cc578168c67e1efcb80ea5`; tracked files remained unchanged. The separately owned concurrent TASK-003 packet is outside this validator's write scope.

Skipped redundant suites, implementation and fresh physical-browser execution. Remaining limit: physical geometry, capture/focus and theme qualification rely on the recorded browser evidence; no new material defect was established.
