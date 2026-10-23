# TASK-027 cumulative invariant review

Result: **PASS**. Proposed finding ledger: **empty**. FIND-027-1 through FIND-027-6 are closed for their diagnosed causes in the candidate source.

## Subject and authority

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`, branch `wyrd-ui-main-integration`. Cumulative base `5e57416d562386066c1aadda9dd01783b9cd4eb5`; candidate `39c15176955edca785cc578168c67e1efcb80ea5`. HEAD was independently checked and remained that candidate. Read the actual base-to-candidate implementation diff, full current renderer/geometry/schema/mount owners, changed host examples and tests, and the latest `7579a2f60..39c151769` diff as a locator. The latter is not the acceptance subject.

Authorities read: root AGENTS.md; spec revision 13, particularly REQ-030/036, AC-011/015 and deferred REQ-031; original TASK-027; changes/spec-driven-development.md; agent_docs/index.md and maintainer-style.md; architecture/README.md and wyrd-design.md client model; TESTING.md; applicable wyrd-ui skill, Svelte runes/testing references and brand/DESIGN.md; Ponytail skill. No nested AGENTS.md exists in the changed UI/example directories. The shared subject records absence of a CodeGraph index. Original task revision-12 layout obligations remain unchanged under approved revision 13.

Read all four prior TASK-027 verdicts, the latest independently validated ledger and R4 task, and traced their diagnoses against current code. Earlier packet conclusions are hypotheses, not source proof. The supplied rebase-preservation result is historical input; this reviewer does not claim a fresh range-diff. TASK-003 changes and flat-identity rebase content were examined at shared renderer/loader/component seams and excluded as independent findings unless layout behavior is affected. This report does not accept TASK-003 as a separate task.

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui/` unless stated otherwise. No other current discovery report was read.

## Value, authority and lifecycle tracing

| Authority / producer | Writers and consumers traced | Invariant assessment |
|---|---|---|
| Host document | Public `doc` -> `ViewDocumentSchema.safeParse` -> full authored signature, layout-stripped content signature, placements, header/error markup and callback serialization | Current authored values govern layout; parsing supplies a separate validated value rather than mutating the caller's document. Replacement reference and nested reactive changes are both observed. |
| Edit state | `key` and pointer `move` -> draft; `commit` -> kept; document reset -> cancel plus empty kept; availability reset -> cancel | Completed edits survive a host that never echoes. A changed authored document supersedes them. Cancellation clears preview and stops its pointer producer. |
| Grid geometry | Current parsed panels plus own retained layouts and own measured heights -> pure `place` -> Map keyed by accepted ID -> boxStyle/current/ordered | Geometry never writes authored layouts. Later collision displacement and intrinsic occupancy remain presentation values. Arbitrary accepted IDs cannot read inherited entries. |
| Measurements | Native offsetHeight binding -> new raw record with computed key -> own-entry read -> rowsFor/place | Continuously measured unplaced content occupies enough fixed-pitch rows. Computed `__proto__` is a real data key; replacement avoids the earlier deep-proxy writer problem. |
| Loaded panels | Content/source/range effect -> start -> panel rows, loading props or per-panel validation problem; asynchronous settle updates current props | The loader generation stays separate from geometry. AbortAll and controller membership reject obsolete settlement. Current errors do not disappear just because props are absent. |
| Narrow reading order | Current placements plus effect-owned panel rows -> membership filter -> stable `(y,x)` sort -> keyed template | Every comparator operand has a placement. Previous IDs are excluded during the scheduling gap before start replaces rows; same-ID loading/error rows remain admitted. |
| Gesture lifetime | Drag installs captured pointer/window listeners; stop removes them and capture; commit/cancel/reset/availability/unmount terminate through release | Every inspected terminal path stops further producer writes. Commit releases even when there is no draft, so a later pointerup cannot duplicate an earlier blur/Enter completion. |
| Hosts | Direct exported WyrdView; mountView props copy; gallery and example document state; AppBridge destroy/remount per tool result | One shared renderer owns editing. MCP-local completed state lasts only for the mounted render. Host echo changes geometry without changing the load key. |

Read full bodies of `current`, `commit`, `cancel`, `key`, `drag` and nested handlers, `abortAll`, `start`, `settle`, `act`, renderer pre/effects, `place`, `edit`, `rowsFor`, `rowsHeight`, `mountView`, `remoteLoader`, applicable loader and Chart sizing paths. Enumerated production renderer callers through repository search: the gallery, example `/view`, and AppBridge mount host; public export also supports custom direct-Svelte hosts. Sibling consumers of geometry are ordering, styling, edit initialization and dimension measurement. Neither the new projection nor reset moves network ownership into those consumers.

## Acceptance matrix

| Requirement, acceptance criterion, constraint, or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Optional whole-unit bounded layout, including x+w; invalid/partial/fractional layouts and width rejected with field context | `schemas.ts:182-201` strict LayoutSchema and PanelSchema; ViewDocumentSchema; validateView/inputProblem | Contract assertions at `src/lib/ui-library.test.ts:651-661`; supplied candidate suite and codegen green | PASS |
| Published view schema carries layout; component inputs stay size-agnostic | Generator consumes ViewDocumentSchema and component schemas; generated catalog diff adds only view layout for TASK-027 | Contract test checks catalog view fields and absence of layout/width/height in component entries; supplied codegen check | PASS |
| 12 columns, fixed 40px tracks and consistent row/gap pitch | `layout.ts` constants, rowsHeight; renderer CSS and boxStyle at `WyrdView.svelte:145-151` | Grid assertions; supplied historical real-browser geometry; no geometry changed in R4 | PASS |
| Unplaced panels full width at intrinsic height below earlier panels; mixed documents and no-layout documents render in order | `place` uses earlier maximum bottom and measured rowsFor; auto box omits fixed height and aligns to start | Grid case covers plain/mixed/intrinsic and changed measurements at test lines 665-731 | PASS |
| Later collisions move downward until clear; rendering never rewrites document | Pure place copies layouts, advances y to overlapping bottom; no write to parsed inputs | Pure and mounted grid assertions plus source immutability checks | PASS |
| Components fill fixed panel boxes; overflow stays inside panel; charts redraw | Shared panel/fill grid, min-size and overflow rules; existing Chart box dimension bindings and flexible body | Original browser resize/overflow evidence and human acceptance retained; source confirms sizing owner preserved | PASS |
| No editing without host handler, or below breakpoint | editable depends on handler and measured width; template gates both affordances | Read-only editing case and narrow case; supplied green suite | PASS |
| Pointer and keyboard move/resize snap and respect bounds | Shared pure edit; drag rounds by column and row pitch; arrows and Shift+arrows use same edit owner | Editing assertions, pointer completion/cancellation assertions; historical native-pointer evidence | PASS |
| Escape cancels; completion emits once; no later producer revives it | release at commit/cancel, stop removes listeners/capture; reset/availability pre-effects and teardown release | Pointer cancellation tests cover focused/unfocused grip/corner, blur/Enter, narrowing, handler removal and reactive authored changes | PASS |
| Whole updated document includes all prior retained edits and leaves untouched authored/auto siblings alone | commit at lines 167-173 overlays all own done entries onto current parsed panels | Non-echo two-panel editing output, untouched sibling and immutability assertions at test lines 778-799 | PASS |
| Accepted arbitrary IDs behave normally | Map placement; Object.hasOwn kept/heights reads; raw immutable records with computed-key writes | constructor/toString/__proto__ editing and measurement regressions, including subsequent growth | PASS |
| Reloading authored document resets local edits; nested changes also end held edits | given plus full parsed authored signature at lines 102-114; cancel shared with all edit terminals | Reactive proxy tests at lines 834-863 and 999-1026; equal-content new reference and unchanged handler toggles distinguished | PASS |
| Layout edits/echo do not reload or reset brush; genuine content/range change retains load lifecycle | Separate content key strips layout; source derives stable loader identity; brush remains independent | Editing and narrow read-count assertions; TASK-003 dependent-load/range and obsolete-result assertions; supplied suite | PASS |
| Narrow views stack full width in resolved `(y,x)` order, retain h and omit editing, including document replacement | width threshold, boxStyle, current-placement membership filter at line 142; stable native sort | Extended narrow case at lines 1030-1094 covers changed IDs, fewer IDs, invalid/valid recovery, loading/error, abort/late result, same-ID input refresh and layout-only reorder | PASS |
| Gallery/example/AppBridge demonstrate layout in supported modes; MCP edits local per render | Gallery callback echo and placed fixtures; example dashboard and `/view`; AppBridge destroys previous mounted view and opts in with local-only callback | Supplied historical desktop/mobile light/dark gallery/artifact evidence, example checks/builds and human acceptance 2026-10-20; current integrated checks/build | PASS |
| No persistence, add/remove/duplicate controls, collapsing, per-viewer storage, component size inputs, layout dependency or shipped-page takeover | Complete attributed implementation diff; local kept state only; no persistence or new dependency path | Static inspection; green checks do not substitute for this source evidence | PASS |

## Prior-finding closure

| ID | Current source evidence | Closure |
|---|---|---|
| FIND-027-1 | commit forms done from every retained entry, then overlays every matching current panel; it does not serialize only the last draft. Untouched auto/colliding siblings keep authored state. | CLOSED |
| FIND-027-2 | commit calls release before its early return; cancel releases; stop removes pointer/window producers and capture. Document/availability pre-effects cancel and teardown releases. | CLOSED for original Escape and revised terminal-entry causes |
| FIND-027-3 | rowsFor/place reserve measured intrinsic rows, CSS tracks remain 40px, and rowsHeight shares pitch. Auto panels keep natural content height rather than stretching a single row. | CLOSED |
| FIND-027-4 | Both retained and height readers use own entries; raw replacement writes preserve reactive measurements for prototype-named IDs; placement uses Map. | CLOSED including measurement writer cause |
| FIND-027-5 | Existing reset observes both reference identity and full validated authored value. It no longer relies on the layout-stripped content key or reference alone. | CLOSED |
| FIND-027-6 | Narrow ordering filters by current placement membership before comparator dereferences. Removed IDs cannot enter comparator; current-ID validation problems/loading/failure are not filtered by result state. IO ownership remains unchanged. | CLOSED |

R4 repairs the first owner joining two valid but differently scheduled inputs. The consumer guard belongs at that join: current parsed placements are authoritative for what may be ordered, while start still owns loading. It needs no loader synchronization, generation store, host remount, fabricated coordinates or schema restriction. The extended regression actually exercises two outgoing rows, so comparator invocation is not avoided by a one-row case.

## Existing-owner and Ponytail assessment

The previous WyrdView already owned panel loading and arrangement. Native CSS Grid alone cannot choose the approved document-order collision push-down, measured intrinsic occupancy and stable narrow reading order, while native Pointer Events do not implement approved snapping or completion serialization. The small pure geometry module supplies those missing policies without another retained owner. State remains with the renderer; schemas remain with the existing schema owner; mountView passes its new callback through. No second parser, workflow loader, graph, transport, cache, persistence layer or dependency was added for layout.

R4 uses the existing Map and native filter/sort. Deleting ordering would violate `(y,x)` behavior; changing start would duplicate or move adjacent lifecycle obligations. Existing tests/facilities suffice. No deletable unearned mechanism or scope expansion with a material consequence was found.

One suspected host seam was falsified with a fresh memory-only installed-Svelte compiler inspection: the example's inline `remoteLoader('/view/load')` compiles into its own dependency-free derived expression, while doc is a separate getter. A layout echo therefore does not itself reconstruct that loader. No output files were created by this inspection.

## Findings and verification limits

Proposed findings: **none**. No correction or specification decision is requested. No optional preference is promoted to a finding.

Verification execution is supplied by the caller at the exact candidate: format, lints, codegen, native TypeScript checks/unit tests and queue tests; UI package/app check with zero errors, 40/40 tests, build, and real-server integration 2/2. Independently inspected test bodies support the mapped invariants. Prior physical browser geometry/focus/capture, theme evidence and human approval remain historical, explicitly distinguished from this static audit. The R4 evidence records expected pre-fix RED and focused GREEN; inspection found no weakened assertion, skip, ignore, timeout or test-policy change to hide failure.

No source/test files were changed and no suites or browser sweep were rerun by this reviewer. Remaining qualification limit: physical geometry and host/theme behavior rely on the recorded browser/human evidence; jsdom assertions establish reactive ordering and lifecycle rather than real pixel layout.
