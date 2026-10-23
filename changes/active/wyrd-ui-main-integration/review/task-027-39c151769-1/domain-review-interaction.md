# Interaction domain review — TASK-027

Result: **PASS**. Proposed material findings: **none**. FIND-027-1 through FIND-027-6 are closed for their diagnosed causes by current source and the inspected regression assertions. This is an independent browser interaction, accessibility and reactive lifecycle discovery review; no current-round discovery report was read.

## Immutable subject and authority

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; cumulative base `5e57416d562386066c1aadda9dd01783b9cd4eb5`; candidate `39c15176955edca785cc578168c67e1efcb80ea5`, also the checked HEAD. Approved specification `changes/active/wyrd-ui-main-integration/spec.md` revision 13; original `tasks/TASK-027-view-layout.md`; latest remediation `review/task-027-7579a2f60-1/TASK-027-R4-narrow-document-replacement.md`. Reviewed the cumulative actual renderer/geometry/schema/host/test changes, not only R4; used `git diff 7579a2f60...39c151769` as the R4 locator. Earlier packet verdicts, ledgers and remediations are hypotheses and historical proof, not stronger authority. PR #205 and TASK-003 changes are excluded as independent findings; Chart/loading/mount seams were inspected for layout regressions.

Governing authorities read: root AGENTS.md; changes/spec-driven-development.md; agent_docs/index.md and maintainer-style.md; architecture/README.md and wyrd-design.md Client model; TESTING.md; applicable approved REQ-030, REQ-036, AC-011 and AC-015; UI brand/DESIGN.md; wyrd-ui skill and its Svelte 5 and testing references. No nested AGENTS.md exists in the changed UI/example paths, and no root CodeGraph index exists. REQ-036 owns local geometry/editing; the client-model boundary forbids promoting this ephemeral presentation state into a durable workflow. Nothing in the task overrides these authorities.

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui/` unless stated otherwise.

## Boundary and source coverage

| Domain boundary | Source inspected | Obligation / assessment |
|---|---|---|
| Host-authored document to accepted local layout | `packages/ui/src/lib/schemas.ts` LayoutSchema, PanelSchema, ViewDocumentSchema; WyrdView.svelte:56–143 | One schema owns bounds; new identities and accepted nested authored changes remain supported; component schemas gain no size input |
| Input event to preview/result | Full WyrdView bodies of current, commit, cancel, key, drag and its move/stop/end/escape closures at 154–233; template controls 315–339 | One local draft/completion owner; keyboard and pointer reuse pure edit geometry; input stays immutable |
| Gesture end and editability lifecycle | WyrdView:102–119, 167–179, 203–233 | Document reference/value reset, handler removal, narrowing, cancellation, completion and unmount all reach the existing release owner |
| Narrow presentation across load generations | WyrdView:121–151, 235–267, keyed template at 304–343 | R4 current-placement membership precedes coordinate reads; asynchronous panel/result ownership stays intact |
| Geometry and arbitrary accepted IDs | Complete layout.ts; WyrdView:121–151, 313 and 368–429; Grid placement assertions at test:665–730 | Own-entry records and Map distinguish arbitrary IDs; continuously measured intrinsic occupancy and fixed pitch remain shared |
| Focus, keyboard alternative and containment | WyrdView:315–339, 391–425; Chart plot/table focus and dimensions | Native labelled button has visible focus and movement/resize keyboard equivalents; corner duplicates functionality, not the only accessible route; panel fill contains overflow |
| Public/sibling consumers | Full view.ts mountView/remoteLoader; app/main.ts; gallery +page.svelte; example panels.ts and view/+page.svelte; renderer caller search | Direct Svelte hosts can replace documents; static mountView forwards the same callback; MCP destroys/remounts each result and retains no durable layout |
| Tests and framework behavior | Layout document contract, Grid placement, Layout editing, Pointer edit cancellation, Narrow views; ui-fixtures.svelte.ts; vitest-setup.ts; installed Svelte effects.js user_effect/user_pre_effect | Tests use native Svelte proxies/scheduling with simulated dimensions; source retains the distinction between render/pre-effects and ordinary load effects |

Existing alternatives were traced before accepting the changed owners. The existing loader/start/abortAll/settle lifecycle already owns read cancellation and stale results. R4 does not duplicate it. Pure layout.ts owns geometry shared by pointer and keyboard; there is no separate validator or drag geometry framework. Native Array projection and current Map membership solve the cross-generation presentation join without new state, identity restrictions, remount protocol or dependency. Local completed edits remain required for a host that does not echo callbacks; deleting kept would remove approved behavior.

## Interaction and lifecycle assessment

| Required reachable path | Source-backed result | Proof assessment |
|---|---|---|
| Host without handler; narrow host with handler | editable derives from handler availability and measured width; both controls are absent when disabled | Layout editing and Narrow views assertions cover both |
| Keyboard move / Shift-resize, complete, Escape | key maps arrows through edit(current(...)); Enter commits; Escape cancels; blur completes through the same commit. edit clamps columns, minimum sizes and maximum authored height | Existing editing assertions inspect preview coordinates, once-only whole output, bounds and Escape restoration |
| Pointer grip and corner with capture | drag cancels prior gesture, snapshots its origin and installs captured movement using grid pitch; move and resize share edit | Pointer assertions cover movement, resize and once-only completion; physical capture remains historical browser proof |
| Escape during held pointer, focused or unfocused | Window Escape listener calls cancel; cancel releases producer listeners/capture and clears draft; later events cannot revive it | Existing Pointer edit cancellation iterates grip/corner and focused/unfocused paths |
| Pointer cancel; blur/Enter completion before or after movement | end removes producers before commit/cancel; commit releases before testing draft, so a no-move completion also disposes listeners | Tests verify inert later movement/up, callback counts, preserved sibling edits and a fresh subsequent gesture |
| Handler loss / narrow transition during active edit | pre-effect calls cancel before controls leave; completed kept entries survive; pending preview does not | Tests exercise focused/unfocused narrowing and handler removal/restoration; source shows shared cleanup |
| Authored replacement / nested accepted host mutation | given tracks reference and authored tracks full validated values; reset cancels gesture and clears kept. content separately omits layout | Native proxy tests demonstrate y7 superseding local retained state, later callbacks preserving y7, range/input writes ending held gestures, same-value handler changes preserving edits and equal-value new references resetting |
| Destroy while gesture/read is active | teardown invokes current release; normal load-effect cleanup aborts controllers; no new persistence owner | Direct source proof; no fresh physical-browser teardown probe claimed |
| Layout-only commit or host echo | content excludes layout; source stabilizes loader identity; commit overlays all retained own entries onto parsed panels without mutating the input | Existing assertions inspect load count, full callback and source immutability; unchanged loading owners remain in place |
| Narrow replacement a/b → c/d or reduced membership | Current placements come from parsed panels, while loaded panels can briefly be older. ordered now filters those older rows using placements.has before comparator dereferences. Stable sort preserves document-order ties | New Narrow views assertions drive at least two outgoing rows through actual mounted Svelte, then reduce membership and recover invalid→valid |
| Pending old read, new loading/failed panel, same-ID refresh | R4 membership is independent of props/result success. start emits current IDs even on validation error. abortAll cancels old controllers and settle rejects aborted/noncurrent controllers | Tests assert aborted old signals, late a result excluded, d loading visible, c failure visible, same-ID input reads, layout-only ordering without reads. Panel-validation-error retention is source-proven from the shared current parsed list |
| Responsive geometry and keyboard focus visibility | Full-width narrow styles omit grid-row and retain explicit rowsHeight; wide fixed tracks and measured auto occupancy remain unchanged. Move focus draws a 2px declaration ring; charts retain accessible focus and table scrolling | Narrow/grid assertions plus historical actual-browser geometry/light-dark evidence; no fresh visual qualification inferred |

No credible new interaction or accessibility regression was established from the cumulative range. The R4 filter guards the join that introduces the invalid comparator input; it does not repeat a guard across hosts or hide current loading/error panels. Keeping ordinary IO effects separate is appropriate: making the loader run in a pre-effect or adding a second generation store would exceed the required correction.

## Prior findings: current-source closure

| ID | Current evidence | Disposition |
|---|---|---|
| FIND-027-1 | WyrdView:170–173 merges kept with the latest completion and overlays every matching own panel; editing test completes a then b without echo and checks both plus untouched siblings | CLOSED |
| FIND-027-2 | commit releases before all exits; cancel releases; stop removes move/up/cancel/window listeners and held capture; pre-effects cover document/editability and teardown releases. Pointer tests cover prior terminal entries | CLOSED, including revised terminal scope |
| FIND-027-3 | rowsFor reserves measured intrinsic rows, place displaces later collisions, CSS uses fixed 40px tracks, height binding updates immutable records, explicit height shares rowsHeight pitch | CLOSED for diagnosed geometry cause; physical measurements remain historical |
| FIND-027-4 | retained/height reads use Object.hasOwn, heights/kept are raw replaced records with computed own keys, placements is Map; tests cover constructor/toString/__proto__ placement, serialization and changed measured height | CLOSED |
| FIND-027-5 | given plus full authored serialization drives existing reset without changing layout-stripped content; native-proxy editing/pointer assertions cover nested writes and stable rerender controls | CLOSED |
| FIND-027-6 | WyrdView:142 filters by current placement membership before stable `(y,x)` sort; Narrow views replacement tests at 1050 onward cover changed membership, reduction, stale reads, loading/failure, recovery and no-reload control | CLOSED |

## Verification provenance and limits

Fresh evidence in this review is independent static source/diff, caller, installed-runtime implementation and regression-assertion inspection. HEAD checked as the exact candidate. No source or test file was written, and no generated output was produced. The only owned write is this report.

Candidate-integrated green checks are supplied by the caller: fmt:check, lints, codegen:check, ts:napi:check, ts:test:unit, wyrd-queue nextest, UI package/app check zero errors, UI 40/40 tests, build and real-server Postgres UI integration 2/2. R4 task evidence records the precise Narrow views RED before the filter and GREEN after it, with focused editing/cancellation/dependent-load checks. These results were not rerun or represented as fresh execution by this reviewer.

Previous packets and original task evidence record actual-browser geometry, focus/capture, AppBridge/light-dark/mobile checks and Steven's gallery approval. They remain historical. R4 changes membership projection, not geometry, focus or event producers; its new mounted scheduling regression directly covers the diagnosed gap. No fresh browser, screen reader, touch-hardware, broad gate, deployment or Linux qualification is claimed.

Final domain result: **PASS**. Active proposal ledger: **empty**. No domain remediation requested.

Skipped duplicate check execution and a fresh browser sweep. Remaining verification limit: current physical browser behavior relies on recorded historical evidence; current scheduling regression execution is caller-supplied and its assertions were independently inspected.
