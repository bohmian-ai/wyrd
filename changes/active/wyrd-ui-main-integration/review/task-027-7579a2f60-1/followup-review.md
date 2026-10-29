# Focused follow-up: narrow replacement generation seam

Result: **RESOLVED**. The four discovery proposals B027-1, I027-R4-01, SYS-027-1 and INTERACTION-1 identify one reachable narrow rendering regression. This report resolves scope and the minimal correction boundary; the separate Ponytail validator must decide the final ledger.

## Subject and focused coverage

Immutable subject: cumulative `5e57416d562386066c1aadda9dd01783b9cd4eb5...7579a2f603c023daae0d15b9d5cc3013eec6f00d`, specification revision 13 and original `tasks/TASK-027-view-layout.md`, as recorded in `subject.md`. HEAD stayed candidate. Read claims-comparison and all four proposal reports, the cumulative renderer/layout/schema/view diff and complete current renderer and mount owners. Applied root AGENTS.md, spec-driven-development, maintainer style, architecture authority map and Wyrd UI guidance. No CodeGraph index or applicable nested AGENTS.md exists in these paths.

This is a focused follow-up, not a second full discovery acceptance audit. UI-relative source locations below start at `crates/wyrd/wyrd-server/wyrd-ui/`.

## Positive scope

REQ-030 explicitly retains custom-app portability and publishes `WyrdView` for a user's own Svelte UI (`spec.md:486-517,750-772`). The renderer comment promises new-document cancellation, and its existing parsed/content/load effects implement document replacement. REQ-036 and AC-015 require narrow views to stack; original TASK-027 scenario 4 imposes no initial-document-only limitation. Different valid documents can have different panel IDs; `ViewDocumentSchema` permits 1–24 unique IDs without a cross-document identity constraint (`schemas.ts:198-227`).

A custom Svelte host assigning another valid document to its renderer is positively supported. The non-goal of adding/removing/duplicating panels excludes new reader editing controls. It does not require a host to keep the first document forever or remount when choosing another document. This finding requires no such editing feature, persistence, new public API or server behavior.

## Producer, consumer and sibling trace

| Owner | Authority and affected paths |
|---|---|
| Host document | `doc` -> `parsed` at WyrdView:56 -> authored layouts and `placements` at 121-134. Placement identity follows current validated document synchronously. |
| Data generation | `content` at 73-75 excludes layouts; the post-render `$effect` at 83-89 calls `start`. Only `start` replaces the panel list (236-254); `settle` changes props on its current rows (257-263). Thus old rows and new placements can coexist during rendering. |
| Presentation join | `ordered` at 137-139 combines those authorities. Narrow sorting dereferences placements for every loaded row, including obsolete IDs; the keyed loop at 300 then consumes it. This is the actual invalid-state producer for the comparator. |
| Sibling geometry consumers | `boxStyle` already accepts a missing placement (141-148). `current` at 152-156 is reached through available edit controls. The proven failing narrow path has no controls. Neither owns panel loading or needs a new lifecycle for this correction. |
| Shared data lifecycle | `abortAll`, `start`, `settle`, range-reset and loader effects already own cancellation, loading/error rows, stale settlement and layout-only load suppression. Their entire bodies were read; none needs to move. |
| Host callers | `view.ts` exports WyrdView and `mountView`; mountView copies initial options and exposes destroy. AppBridge destroys/recreates its mount per result (`src/app/main.ts:24-26`). Gallery and example directly use WyrdView and echo layout callbacks. The public direct Svelte host permits full reactive document replacement, unlike static mountView options. All share the same presentation owner. |

Neither the R3 authored reset nor its gesture cancellation promises to atomically replace loaded rows. Expanding it to start loads would couple layout reset to IO and risk the no-reload invariant. Existing geometry helpers have no asynchronous state and cannot solve this generation join.

## Independent actual-candidate reproduction

Ran the installed Svelte client compiler/runtime with actual unmodified WyrdView, jsdom, rolldown and a native Svelte proxy, through `mise exec -- node --input-type=module` from the UI directory. Compilation, bundling, entry module and data-URL import remained entirely in memory. No probe/source/test/generated file was written. The system-review memory-only bundling recipe was reused with a deferred recording loader and assertions rather than assuming scheduler behavior.

A valid document used `version: wyrd.view/v1`, `catalogId: wyrd.ui/v1`, range `{last:'6h'}`, two Chart panels a/b, stat input `{type:'stat', title:'Count', sql:'SELECT count(*) AS n FROM t', y:['n']}`, and bounded layouts `{x:0,y:8 or 0,w:6,h:4}`. The source schema accepted the document. Width was 500, height measurement 100. Mounted with reactive `proxy({doc,load,onlayout:()=>{}})`, flushed, replaced doc with the identical document shape using IDs c/d, then flushed.

Observed on the unmodified candidate:

```text
candidate ["distinct/500: Cannot read properties of undefined (reading 'y')"]
```

This is the actual mounted renderer, not an isolated comparator or scheduler model. At least two previous rows force comparator evaluation. No gesture or completed local edit is necessary. A layout handler was supplied in this follow-up, but narrow mode exposes no controls; prior independent runtime evidence also reproduces without a handler.

## Minimal correction boundary and in-memory falsification

Select the existing narrow reading-order projection as the correction owner. Exclude loaded rows without membership in the current placement Map before comparing placement coordinates. This is an ordinary join of available asynchronous rows to current authored geometry. Rows with a current identity remain regardless of whether their component is loading, invalid, denied or failed. Do not filter by props/result state. Keep the wide branch unchanged.

I tested exactly that approach by altering the source string only while the memory compiler loaded WyrdView: the narrow `panels.toSorted` receiver became `panels.filter((panel) => placements.has(panel.id)).toSorted`; all other source remained candidate code. This is evidence for the boundary, not a prescribed private implementation or a source edit.

| Memory-only mounted scenario | Assertions and result |
|---|---|
| Narrow a/b -> c/d | PASS: only d/c in resolved `(y,x)` order, full width, retained 196px explicit height, no move controls; four loads total. |
| Narrow a/b -> a | PASS: only a, same geometry, three loads total. |
| Narrow valid -> invalid -> valid c/d | PASS: invalid document shows error and zero panel rows; next valid document shows d/c and starts its ordinary loads. |
| Same IDs, changed input title | PASS: existing IDs remain b/a in geometry order, four loads total; loader generation still refreshes input. |
| Layout-only new document value, same data | PASS: layout order updates to a/b, two loads total; loading authority does not rerun. |
| Deferred obsolete result after replacement | PASS in each non-layout narrow case: first generation signals aborted; resolving it with `OLD / Obsolete result` never renders that text. |
| Current loading then failed result | PASS in each non-layout narrow case: current rows render loading before settlement, then exactly the current row count of error states with `CURRENT / Current failure`; no loading rows remain. |
| Wide a/b -> c/d, handler present | PASS: c/d document order, four loads total, obsolete signals aborted, two edit handles. |

The loading/error assertions prove filtering does not mistakenly discard unsatisfied loads. Per-panel validation-error rows also have IDs from `view.data.panels` in `start` and corresponding placements regardless of props, so membership retains them by construction; this branch was source-traced rather than separately mounted. Same-ID reloads continue to use the existing data lifecycle; there is no need to remove a row merely because its props belong to the outgoing data load.

Probe limits and incidental failures: the first command attempted original and variant runtimes in one process after the deliberately caught candidate render error; the candidate error left scheduled work and emitted a long data-URL stack, so variant results from that command were not used. The independent fresh-process variant run passed all five narrow scenarios. Its final wide attempt lacked jsdom's HTMLMediaElement global and stopped in native event handling; a separate fresh process supplied HTMLMediaElement/SVGElement and passed wide replacement/unmount. These are probe-environment failures, not additional candidate findings. No actual-source test or broad check was changed to clear them.

## Why broader synchronization is unnecessary

The placement Map already expresses exactly which identities are current. Using it at the join prevents stale rows from entering the comparator; no fallback coordinates, generation token, duplicate state, parallel loader, network work in pre-effects, per-host remount workaround or reset rewrite is required. This consumer owns the invariant because it is the first place the independently valid streams are combined. It is not a repeated downstream symptom guard.

The current `start` still produces all new loading and validation-error rows; controllers still abort and prevent late writes; same-ID content updates still reload; layout-only changes still do not. The shared source correction reaches exported WyrdView and all hosts without changing their contracts. Broader generation synchronization would move unrelated loading behavior solely to protect this presentation join and has no demonstrated need in this task.

## Retained proposal and closure proof

Retain the four proposals as one **REGRESSION** at `WyrdView.svelte:137-139`: replacing a valid document at narrow width can crash before its ordinary load effect completes because the reading-order projection compares obsolete IDs against current placements. Observable consequence: a supported custom Svelte renderer fails on valid new authored input. Correction outcome: narrow ordering consumes only rows with current authored geometry, preserves `(y,x)` ordering and every current row state, and leaves existing IO/edit lifecycle intact.

The smallest credible persisted regression proof extends the existing narrow component test with a valid two-panel document replaced by different valid IDs, plus a removal transition. Assert no exception, current-only order/explicit geometry/read-only state, and ordinary loading/error completion. A deferred first-generation read proves abort/stale rejection; preserve same-ID and layout-only no-reload behavior with existing facilities. Invalid-to-valid recovery and wide replacement are focused regression controls for this shared projection, not new features. No new harness, dependency or broad server journey is needed.

No additional independent finding was discovered. FIND-027-5 remains a distinct closed reset-dependency cause; this report does not reopen it or renumber prior findings. The final validator must independently assess the retained shared cause and correction.

Only this report was written. Skipped persistent implementation, broad suites and real-browser geometry; runtime evidence uses installed Svelte with simulated DOM measurements.
