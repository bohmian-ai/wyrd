# TASK-027 maintainer review

Result: **PASS**. No material maintainer findings are proposed.

## Immutable subject and method

Reviewed cumulative base `5e57416d562386066c1aadda9dd01783b9cd4eb5` through candidate `39c15176955edca785cc578168c67e1efcb80ea5`, against specification revision 13, original TASK-027, and the prior FIND-027-1..6 diagnoses and R1–R4 corrections. HEAD was still the candidate at completion of source inspection. Latest R4 was a locator; the cumulative layout implementation, its consumers, and the earlier corrections were also inspected. No other current-round discovery report was read.

Authorities inspected: root `AGENTS.md`; `changes/spec-driven-development.md`; `agent_docs/index.md` and `maintainer-style.md`; architecture authority map and relevant client/presentation boundary; `TESTING.md`; the UI skill, Svelte 5 reference, and `brand/DESIGN.md`; specification REQ-030, REQ-036, AC-011 and AC-015. The subject establishes that no `.codegraph/` index exists. No nested `AGENTS.md` was found in the affected UI/example directories.

The range also contains TASK-003 foundation, flat-identity, bounded-query, chart and action-state changes and prior review/planning documents. Those were surveyed for interaction with layout, not treated as independent TASK-027 findings. In particular, the query-result/native SDK changes do not own placement or edit state; chart and CardHeader changes remain children of the existing panel box. Unrelated PR #205/main content is excluded.

## Changed-surface coverage

Locations below are relative to `crates/wyrd/wyrd-server/wyrd-ui/`, except explicitly named example paths.

| Surface and inspected consumers | Maintainer assessment and evidence |
|---|---|
| `packages/ui/src/lib/schemas.ts:178-225`; generator `scripts/catalog.ts`; generated `catalog.json`; `validateView` | PASS. Layout is a strict optional panel field with one Zod owner. Existing view parsing and catalog generation consume that owner; component input schemas remain size agnostic. `Layout` and `ViewDocument` are inferred from the schemas rather than redefined contracts. Bounds and malformed-field behavior are directly asserted by Layout document contract. |
| `packages/ui/src/lib/layout.ts:1-52`; renderer placement/style/current/edit callers | PASS. The new module has one coherent, pure responsibility: grid geometry. `place` consumes caller-owned layout/height values, copies authored boxes, and resolves occupancy; `edit` provides the common bounded operation for pointer and keyboard. The functions neither retain renderer state nor form a parallel runtime owner. Grid pitch helpers explain the gap-inclusive height model. |
| `packages/ui/src/lib/WyrdView.svelte:56-146`; existing loading effect, `start`, `abortAll`, `settle` | PASS. The original renderer remains the one lifecycle owner. Layout-independent `content` keys panel reads; `given` plus complete validated `authored` values reset local layout state. IO still starts in the existing post-render effect and its controllers remain the generation authority. R4 projects current identities at the point where current placement and effect-owned loaded rows meet. |
| Renderer `boxStyle`, `current`, `commit`, `cancel`, `key`, `drag` at lines 146-235; template and CSS | PASS. State producers, completion, cancellation and listener cleanup are adjacent to their owning component. Completion merges all retained edits into the full current document while keeping rendering displacement out of authored output. Pointer and keyboard share `edit`. The listener closure owns its capture/listeners and is released through the same terminal operation from completion, cancellation, reset and teardown. Panel fill/overflow and container-width behavior are local presentation concerns. |
| `packages/ui/src/lib/view.ts:26-46`; `packages/ui/src/app/main.ts:22-27` | PASS. `onlayout` is typed in both renderer Props and mounting options and forwarded by the existing mount spread. There is no new host protocol or persistence owner. MCP App replacement destroys the prior renderer, while the no-op callback intentionally enables the renderer's existing per-render retained state. Public source types express the callback document contract; package export/build machinery continues to generate declarations from these sources. |
| Gallery `src/routes/_gallery/+page.svelte`; `examples/ui-app/src/panels.ts`; `examples/ui-app/src/routes/view/+page.svelte` | PASS. Existing hosts provide example authored boxes and keep callback documents in local Svelte state. They do not duplicate geometry, gesture handling or persistence. Header row sizing remains an example authoring choice. |
| `src/lib/ui-library.test.ts:641-1093`; `src/lib/ui-fixtures.svelte.ts`; `vitest-setup.ts` | PASS. Existing scenarios assert caller-visible output, nonmutation, load counts, panel ordering and gesture terminal behavior. The small rune fixture supplies actual native Svelte proxy semantics, which a plain object fixture cannot represent. The observer stub supplies missing jsdom measurement notifications rather than reimplementing geometry. R4 extends the existing Narrow views scenario, with deferred results and real renderer scheduling, instead of adding another harness. |
| Chart/CardHeader and loader seams in the cumulative diff | PASS for layout interactions. Chart retains its existing measured plot box (`Chart.svelte:18-26`); panel `.fill` supplies containment without chart-size inputs. Loader and error changes continue through existing validation/load contracts and do not create a second layout or reload mechanism. |

## Existing owners and alternative-mechanism audit

The former `WyrdView` auto-fill CSS grid was the existing panel arrangement owner. Native CSS grid remains the rendering mechanism, but cannot alone provide the specified document-order collision push-down, measured intrinsic occupancy, and whole-document edit callback. The bounded pure geometry module is therefore earned; it does not replace or duplicate a repository graph, cache, transport or loader workflow. A source search of UI/package/example consumers found the renderer as the only production caller of the geometry operations and the gallery, example and MCP App as its host consumers.

The existing Chart brush also handles pointer input, but owns a distinct SQL time-range action and SVG-coordinate conversion. It does not own dashboard positions, edit callbacks or panel cancellation. Reusing its complete workflow would conflate those behaviors. The candidate instead reuses browser pointer capture, Svelte lifecycle/measurement and CSS grid directly, with no gesture library or generic interaction layer.

R4 is especially small: `WyrdView.svelte:143` filters by `placements.has(each.id)` before sorting a fresh array. The Map already represents the current document's membership. Filtering by this identity preserves rows with loading, failure or validation-error state; it does not inspect props or result success. The remaining comparator assertions follow that same synchronous membership predicate. The correction leaves wide rendering, controller generations and IO timing with their established owners rather than adding synchronization machinery.

## Prior findings, independently inspected closure

| Prior ID | Current source and focused proof inspected | Assessment |
|---|---|---|
| FIND-027-1 | `commit` merges `kept` and serializes all matching parsed panels; Layout editing checks sequential edits without host echo, untouched auto/collision siblings, input immutability and no reload after echo. | Closed. |
| FIND-027-2 | `commit` releases before returning; `cancel`, reset/editability pre-effects and teardown also release; `drag.stop` removes pointer/window listeners and capture. Pointer edit cancellation exercises Escape, pointercancel, blur, Enter, handler loss, narrowing, authored writes and subsequent inert events. | Closed for the diagnosed terminal paths. |
| FIND-027-3 | `rowsFor`/`place`, fixed CSS tracks, `rowsHeight`, auto `align-self` and offset-height binding consistently separate content height from occupied rows. Grid placement checks initial and changed measured occupancy. | Closed; physical geometry proof remains the historical browser evidence. |
| FIND-027-4 | Own-entry checks protect both retained layouts and measured heights; immutable computed-property writes and Map placements preserve arbitrary IDs. Tests exercise constructor, toString and `__proto__`, including changing measurements and valid callback documents. | Closed. |
| FIND-027-5 | `authored` observes complete parsed values while `given` tracks document replacement. Layout editing and pointer cancellation use the native rune fixture to change authored layout/input/range in place and verify reset, callback values and load isolation. | Closed. |
| FIND-027-6 | Narrow `ordered` excludes obsolete IDs before comparison. Existing Narrow views now replaces a/b with c/d at 500px, reduces membership, checks current loading/failure states, aborted outgoing reads and late-result rejection, recovers invalid-to-valid, and distinguishes content from layout-only updates. | Closed by the selected projection-owner correction and persisted regression proof. |

## Findings and verification limits

Material proposed findings: **none**. Non-blocking preferences: none recorded; no style-only correction is requested.

Independent work here was static source/caller/test inspection and a clean cumulative `git diff --check`. Candidate-integrated format, lint, codegen, N-API, TypeScript, queue, UI checks, 40/40 UI tests, build and real-server integration 2/2 are supplied verification, not freshly rerun results. The original task records human gallery acceptance and earlier real-browser geometry evidence; no new physical browser sweep or generated-declaration rebuild was run for this report. R4 changes membership projection rather than geometry or public declaration shape, and its persisted scenario directly targets the prior scheduling failure.

Only this report was written. Source and tests were not changed; no commit or push was performed.
