# Structured Ponytail validation — TASK-027

Result: **one independently confirmed active finding, FIND-027-6**. Discovery proposals B027-1, I027-R4-01, SYS-027-1 and INTERACTION-1 describe the same narrow presentation-generation join. FIND-027-1 through FIND-027-5 remain closed for their diagnosed causes. No new product or public-contract decision is required for correction.

## Immutable subject and independent coverage

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; cumulative base `5e57416d562386066c1aadda9dd01783b9cd4eb5`; candidate and final checked HEAD `7579a2f603c023daae0d15b9d5cc3013eec6f00d`. Authority: approved `changes/active/wyrd-ui-main-integration/spec.md` revision 13, original `tasks/TASK-027-view-layout.md`, root AGENTS.md, architecture authority map and client-model boundary, spec-driven-development, maintainer style, applicable UI skill/rune/testing guidance and TESTING.md. No CodeGraph index applies. Current revision 13's flat Card identity is authoritative; unrelated PR #205 and independent TASK-003 behavior are excluded.

Read subject.md, both complete task discovery reports, standards-review.md, maintainer-review.md, system-review.md, domain-review-interaction.md, claims-comparison.md, followup-review.md and verification.md. Inspected cumulative renderer/layout/schema/mount/host/chart diffs, complete current renderer and geometry owners, schema definitions, mounting and remote-loading functions, all production renderer callers, affected tests and prior finding/remediation records. The rebase-preservation result is supplied orchestrator/invariant-review evidence, not a fresh range-diff claimed by this validator. Source and test files stayed immutable; only this report was written.

The current standards and maintainer reports propose no material issue. Their PASS results establish their respective inspected rules and structure; they do not establish atomic synchronization between the renderer's placement and loaded-panel authorities. The focused follow-up narrows the source correction correctly, but its experiments are not substituted for the fresh reproduction below.

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui/` unless explicitly stated.

## Proposal dispositions

| Proposal / claim | Independent disposition | Stable result |
|---|---|---|
| B027-1: narrow replacement sorts previous rows against new placements | **CONFIRMED** by actual current-source mounted reproduction and producer trace | FIND-027-6 |
| I027-R4-01: missing current identity at the narrow comparator | **CONFIRMED**, same authority and consequence | Duplicate of FIND-027-6 |
| SYS-027-1: mixed generations interrupt the mounted view before ordinary replacement completes | **CONFIRMED** for the browser-component boundary; choose the local presentation join rather than changing load synchronization | Duplicate of FIND-027-6 |
| INTERACTION-1: read-only narrow custom host fails while wide replacement succeeds | **CONFIRMED**; editing and retained state are unnecessary | Duplicate of FIND-027-6 |
| Focused follow-up selects current-placement membership at narrow ordering and preserves IO lifecycle | **CONFIRMED** independently by full owner tracing and fresh memory-only variant scenarios | Selected correction boundary for FIND-027-6 |
| Current-source closure claims for FIND-027-1..5 | **CONFIRMED for each diagnosed cause**, with evidence below | Closed; no renumbering or reopening |

There are no rejected finding proposals to carry forward as optional advice, no human-directed additions, and no distinct finding per host, event, removed ID or asynchronous result.

## FIND-027-6 — CONFIRMED / REGRESSION

**Discovery sources:** B027-1, I027-R4-01, SYS-027-1, INTERACTION-1; focused follow-up.

**Violated obligation:** approved REQ-030 publishes `WyrdView` for a user's own Svelte application; REQ-036/AC-015 require narrow rendering in resolved `(y,x)` order, retaining explicit height, and authored document reload behavior. Original scenario 4 is not limited to initial mount. The non-goal against adding/removing/duplicating panels excludes new reader editing functionality; it does not revoke the existing public host's ability to pass a different valid complete document. Current source already implements document replacement, and TASK-003's existing replacement test exercises changed panel membership. The proposed correction adds no editing feature.

**Exact location:** `packages/ui/src/lib/WyrdView.svelte:137-139`, particularly the placement dereference at line 138. Producing dependencies: parsed document at 56, content/load effect at 73-89, placements at 129-134, and panel-array writer `start` at 236-255.

**Reachable failure:** a direct Svelte host renders a valid document with two panel IDs a/b at measured width 500px. It assigns another valid document with IDs c/d. Current parsed/layout/placement derivations immediately describe c/d. The loaded `panels` collection still holds a/b until the ordinary post-render loading effect calls `start`. Narrow `ordered` runs its comparator with these previous rows, reads `placements.get(a.id)` and `placements.get(b.id)`, and dereferences undefined `.y`. TypeScript non-null assertions supply no runtime check. The render throws before the new generation completes normally. At least two outgoing rows make comparator invocation deterministic; no handler, edit, gesture, remote dependency or race between network results is required.

**Observable consequence:** a supported custom narrow Svelte host throws an uncaught render TypeError when given another schema-valid document instead of displaying its authored panels. This interrupts the mounted view, not a shared server process. Static mountView/MCP remounting avoids this transition, but does not remove the positively supported direct-Svelte caller. Existing green initial narrow tests never replace document membership.

### Full producer, writer and sibling-consumer trace

| Authority / owner | Writers and consumers inspected | Consequence for correction |
|---|---|---|
| Host document / parsed | Public doc prop -> strict ViewDocumentSchema -> layouts/placements, authored reset, content key, header/error template. Schema admits 1–24 locally unique arbitrary nonempty IDs and permits successive documents with different sets | New placements are valid; rejecting new IDs or imposing cross-document identity would change the contract |
| Layout state | Keyboard and pointer produce draft; commit replaces kept; authored/given reset cancels draft and clears kept. layouts combines only current parsed rows, own completed entries and draft | FIND-027-5 reset now works; it cannot make separately scheduled loaded rows atomic and should not own panel IO |
| Loaded collection | Only start replaces panels: invalid document -> empty list; valid document -> one row per current authored panel, including loading or panel-validation error. settle only changes current row props after controller checks | Preserve existing data lifecycle; filtering by loading/props/result success would wrongly hide required states |
| Read generations | Load effect cleanup and start invoke abortAll; start installs controllers and starts reads; settle rejects aborted/non-current controllers, then finds the current row. act handles existing range/action state | No new controller, cancellation owner, token, store or loader is needed |
| Reading-order join | ordered combines effect-owned panel rows with current placement Map; keyed template consumes it | This is the first owner that creates the invalid comparator input. A current-identity membership condition belongs here |
| Sibling geometry consumers | boxStyle returns an empty style for absent placement; current reads placement for edit controls; height binding writes own raw entries consumed by placements; pure place/edit helpers retain no generation state | Narrow mode has no edit controls. No demonstrated reason to change these owners or geometry rules |
| Callers | view.ts exports WyrdView directly and mounts a static options copy; gallery and example directly pass document state and echo layout output; AppBridge destroys/remounts per result | Shared renderer correction reaches every host. Per-host remounts or ID restrictions would add workaround behavior |

Read the full bodies of ordered, boxStyle, current, commit, cancel, key, drag and its closures, abortAll, start, settle, act, all reset/load effects, place/edit, mountView and remoteLoader. The correction neither moves nor duplicates their responsibilities. The system proposal's broader synchronization wording is resolved to this presentation join; synchronizing or moving network effects is unnecessary for the proven gap.

### Fresh independent runtime evidence

Executed the actual unmodified candidate WyrdView with installed Svelte compiler/client runtime, jsdom and rolldown through `mise exec -- node --input-type=module` in the UI directory. A browser-conditioned bundle was generated in memory and imported from a data URL; no module, probe, generated or test file was written. Native Svelte `proxy` props drove mount and replacement; `flushSync` exercised the actual renderer. Mock clientWidth=500 selected the narrow branch; offsetHeight=100 supplied deterministic measurement. A pending loader ensured the failure did not depend on network settlement.

Documents used version `wyrd.view/v1`, catalogId `wyrd.ui/v1`, range `{last:'6h'}`, two Chart panels with stat input `{type:'stat',title:'Count',sql:'SELECT 1 AS n',y:['n']}`, bounded layouts `{x:0,y:8 or 0,w:6,h:4}`, and IDs a/b then c/d. The actual ViewDocumentSchema accepted both documents. Output:

```text
valid true true
before [ 'b', 'a' ]
actual candidate failure TypeError Cannot read properties of undefined (reading 'y')
```

This proves the real candidate path, not merely a hand-written comparator or inferred scheduling model. The command caught the expected error deliberately; its successful exit is not a passing application result.

In a separate fresh process, tested a source-string-only variant that adds current placement Map membership to the existing narrow panel projection before sorting. All other renderer/loader source stayed unchanged. Independent assert-based mounted scenarios passed:

| Variant scenario | Actual assertions |
|---|---|
| a/b -> c/d at 500px | No throw; only d/c in resolved order; 196px explicit height; no handles; four reads |
| a/b -> a at 500px | No throw; only current a; explicit height/read-only; three reads |
| Valid -> invalid -> valid c/d | Invalid input renders no panel rows; subsequent valid input recovers normal new ordering and reads |
| Same IDs, changed component input title | Refreshes ordinary reads; preserves b/a geometry; same-ID replacement remains supported |
| Layout-only document value | New layout order a/b; still exactly two reads; preserves explicit height/read-only behavior |
| Delayed obsolete reads, in all non-layout cases | Initial signals aborted; resolving old results with OBSOLETE never renders obsolete text |
| Current generation loading then failure | Current Loading remains visible before settlement; CURRENT failure remains visible afterwards |
| Wide a/b -> c/d with handler | Unchanged wide branch; new document-order c/d, normal four reads, explicit height and old-generation rejection |

The same variant process passed all six scenario iterations and clean unmounts. Panel-validation-error retention is additionally source-proven: start emits its ID from the same current parsed panel list that supplies placements, so membership retains the row regardless of missing props/problem state. This particular error branch was not separately mounted by the validator. The variant is feasibility and correction-boundary evidence, not an implementation change or closure of the immutable candidate finding.

### Smallest safe correction and deletion/reuse ladder

1. **Cannot delete required ordering or replacement.** Narrow `(y,x)` order and published direct-Svelte hosting are approved behavior. Deleting sorting, forcing remounts, or freezing host documents would remove required behavior.
2. **Reuse current authority.** The existing placement Map already gives exact current authored identities, including unplaced panels. No existing load helper makes render and post-effect timing atomic. R3's reset solves a different lifetime and should remain independent of IO.
3. **Use the conventional local join.** Narrow reading-order projection should admit only available loaded rows whose identities exist in current authored geometry before reading coordinates. Exclude obsolete IDs; retain every current-ID state. Stable sort preserves existing document-order ties. Do not fabricate coordinates or retain old placement snapshots.
4. **Native platform suffices.** Existing Array filtering/sorting and Map membership express this join. No dependency, global store, new generation state, lifecycle framework, schema rule or host protocol is justified.

**Selected decision-complete correction:** repair the shared WyrdView narrow ordering/projection owner using the existing current placements membership. Preserve the wide branch and every existing loading, validation-error, failed-result and current-ID row. Keep start/abortAll/settle, separate layout-stripped content, full authored reset, range behavior and gesture cleanup intact. Rows with no current identity must not enter the narrow comparator; new loading rows arrive through the existing start effect. This consumer guard belongs here because the join itself owns the cross-generation presentation invariant, while each input is independently valid. It is one shared source correction, not repeated downstream guards.

**Preserved adjacent behavior:** same-ID input/range changes reload normally; layout-only edits/echo do not reload or reset brushed range; obsolete result rejection remains controller-owned; current loading/error/invalid-panel states remain visible; current authored layout supersedes local state; fixed pitch, render-only collisions, arbitrary IDs, retained callbacks and once-only gesture completion remain unchanged. No server, catalog, public API, schema or persistent-data decision is necessary.

### Focused closure proof

Extend the existing narrow component case/facilities, with at least two outgoing rows, to replace a valid document with different valid IDs and then a membership-reducing valid document. Assert no thrown render error, only current IDs in `(y,x)` order, explicit height and full-width style, and no edit controls. Record ordinary new reads and keep one outgoing read pending so cancellation and late-result rejection are exercised; assert current loading and failed states remain renderable. Include same-ID and layout-only no-reload controls using existing tests/facilities. Existing document-invalid recovery and wide replacement are narrow regression controls for the shared projection, not additional product features.

Run the exact affected Narrow views selector, Layout editing and Pointer edit cancellation selectors, and TASK-003 Dependent loads and safe actions selector through mise, followed by UI check/build for the renderer/test write set. Grid placement remains an appropriate focused regression if geometry is touched; this selected correction requires no geometry change. No schema regeneration, new harness/dependency, broad aggregate or full backend journey suite is needed for this bounded remediation. Existing tests and the memory-only probe do not constitute persisted regression coverage until the implementation adds and runs the test.

## Prior finding closure from current source

| Stable ID | Independently inspected evidence | Disposition |
|---|---|---|
| FIND-027-1 | commit at 163-170 creates done from all kept own entries and overlays every corresponding parsed panel; non-echo a/b completion test verifies callback two, untouched colliding/auto siblings, immutability and echo without reload | CLOSED; serializer cause removed |
| FIND-027-2 | commit releases before early return; cancel releases; drag stop removes pointer/window producers and capture; pre-effects cover document and editability transitions; teardown releases; existing tests cover held blur/Enter/availability and reactive authored cancellation | CLOSED for original and revised terminal-entry causes |
| FIND-027-3 | rowsFor/place reserve measured intrinsic occupancy; fixed 40px tracks and rowsHeight share pitch; auto box retains intrinsic height; native offsetHeight binding continuously replaces height record; tests cover positive/changed measurements | CLOSED; physical geometry qualification remains historical browser evidence |
| FIND-027-4 | Object.hasOwn retained/measurement reads, raw immutable computed-property height writes and Map placements; tests cover constructor/toString/__proto__, output validity and subsequent measurement changes | CLOSED including writer/reactivity cause |
| FIND-027-5 | authored serializes full validated parsed value while given tracks replacement identity; existing reset observes both, cancels and clears kept; native proxy tests show authored layout y7 superseding local edits, subsequent output preserving it, and held input/range writes cancelling | CLOSED; layout-only load key remains separate |

FIND-027-6 needs no retained edits or active gesture and would still fail if kept/draft were empty. It is a distinct comparator-domain error rather than failed R3 reset dependency, so no prior ID is reopened.

## Final ledger and limits

| Active stable ID | Status | Classification | One-line summary | Correction owner |
|---|---|---|---|---|
| FIND-027-6 | CONFIRMED | REGRESSION | Narrow valid-document replacement compares obsolete loaded IDs with current placements and throws | Existing WyrdView narrow ordering projection, current placement membership |

All proposed discoveries reduce to this one ledger row. A bounded remediation is required. No additional finding or speculative hardening request is retained.

Caller-supplied candidate checks remain green: package/app check zero errors, 40/40 tests, build, real-Postgres UI integration 2/2. They do not cover narrow membership replacement. Validator evidence is fresh source tracing and actual installed-runtime memory-only reproduction/control probes. Browser CSS geometry, physical capture/focus, themes and human acceptance remain historical evidence. No broad gate, fresh native-browser sweep, Linux build, release or deployment proof is claimed. HEAD remained the specified candidate.

Skipped implementation, duplicate integrated suites and fresh browser execution. Risk: the demonstrated narrow replacement failure remains in the immutable candidate until the bounded source correction and persisted regression test pass.
