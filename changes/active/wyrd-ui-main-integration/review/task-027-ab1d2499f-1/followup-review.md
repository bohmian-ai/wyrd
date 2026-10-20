# TASK-027 focused geometry follow-up

Result: **RESOLVED**. The mixed-layout geometry proposal B027-03 is supported by source and a fresh browser measurement. This resolves only the conflict identified in `claims-comparison.md`; it is not another full task review or final finding validation.

## Immutable subject and scope

Repository and commits are those in `subject.md`: base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`, candidate/unchanged HEAD `ab1d2499f6a08a6b5afe1f6c904e72746d526bda`. Inspected cumulative changed geometry in `git diff a1e9b48d2...ab1d2499f`, its complete owning placement/edit functions, renderer placement and measurement consumers, component intrinsic sizing, mountView/AppBridge/gallery/example consumers, and the grid-placement scenario. Read only B027-03 and the specified geometry matrix claims from the two discovery reports.

Authority: AGENTS.md, agent rules, reference router, spec-driven development and maintainer style, approved spec revision 12 REQ-036/AC-015, original TASK-027 scenarios 2–3, and applicable brand geometry guidance. No CodeGraph index exists. The question is physical placement under the approved document language, not a preferred CSS implementation.

## Authority interpretation

REQ-036 says “rows are 40 CSS px”; it separately permits intrinsic content-height unplaced panels and mixed documents. Neither the task nor approved spec changes rows to minimum heights when an unplaced panel appears. The candidate also explicitly commits to fixed geometry through `ROW=40`, `GAP=12`, `rowsHeight`, and pointer movement divided by `ROW+GAP`. Thus, with the candidate's 12px gutter, an unchanged noncolliding panel at `y=4` must begin 208px below grid origin. Intrinsic content must consume an appropriate extent on that grid; it cannot redefine the coordinate pitch for later authored placements. No new public layout semantics or specification revision is needed to satisfy both requirements.

## Producer, consumers and browser evidence

`packages/ui/src/lib/layout.ts:28–35` assigns an unplaced panel `{y: bottom, h:1, auto:true}` regardless of content extent. `WyrdView.svelte:108–122` uses that rectangle for placement and emits a one-row span with no height for auto panels. Its `.panels` rule at lines 310–315 uses `grid-auto-rows:minmax(40px,auto)`. Intrinsic CardHeader or Chart content therefore grows that single grid track. Later row starts include the expanded track, but `rowsHeight` and `drag` continue using fixed pitch. `current` at lines 125–130 measures auto height only when beginning an edit, so it cannot repair ordinary mixed rendering.

Fresh browser checks reused the already running AppBridge harness at `http://localhost:5190/`, cached Playwright 1.63.0, and existing cached Chromium headless shell 1248. No dependency, harness file, test file or implementation was added. Invocation was `mise exec -- node --input-type=module` with an inline script importing the cached Playwright module. A response interception exposed the existing harness's bridge in that one browser page; the script then called its existing `sendToolResult` with valid documents. No application code or CSS was intercepted or changed.

The served artifact and `packages/ui/dist/wyrd-view.html` had identical SHA-256 `d148e53357bbd899be1f3884ab3d8e471d8b06bf9898a4bd23c286d8645db351`. The rendered row styles match candidate source. At a 1200×1000 viewport the script measured each `.panel` using `getBoundingClientRect`, relative to `.panels` origin, and inspected computed `gridTemplateRows`. Both Chart fixtures used the existing valid timeseries query/input and fixture executor; measured runs had no component problems.

| Document/action | Computed tracks | Auto panel height | Fixed panel row/top/height |
|---|---|---:|---|
| Unplaced CardHeader, then Chart `{x:0,y:4,w:12,h:4}` | `72.1875px 40px 40px 40px 40px 40px 40px 40px` | 72.1875px | `5 / span 4`; **240.1875px**; 196px |
| Same view, focus header grip, ArrowRight then Enter; full-width header remains x=0 but becomes measured fixed h=2 | Eight 40px tracks | 92px fixed | unchanged `5 / span 4`; **208px**; 196px |
| Unplaced CardHeader, then same fixed Chart authored at y=0 | `72.1875px 40px 40px 40px 40px` | 72.1875px | collision pushes to `2 / span 4`; **84.1875px**; 196px |
| Unplaced valid timeseries Chart, then Chart `{x:0,y:4,w:12,h:4}` | `294px 40px 40px 40px 40px 40px 40px 40px` | 294px | `5 / span 4`; **462px**; 196px |

The successful inline assert checks establish that the mixed fixed panel top exceeds `4*52`, fixed h=4 still measures 196px, converting the auto header makes the unchanged fixed panel start exactly at 208px, and the collision case clears the actual auto height plus 12px. This isolates the mismatch: physical coordinate drift is proven; an overlap is **not** claimed from these cases. In particular, current CSS preserves separation by stretching the track, rather than satisfying fixed-grid coordinates.

All hosts share this source: `view.ts::mountView` mounts WyrdView; the AppBridge artifact uses mountView with a per-render handler; gallery and custom example use WyrdView directly. Components are intrinsic-content producers, not geometry authorities. `place`, `boxStyle`, `current`, and `drag` are the affected shared consumers. The mixed grid-placement test in `src/lib/ui-library.test.ts:388–414` checks style strings and input immutability; its DOM environment cannot observe computed track sizing. Existing gallery/example documents and previous visual acceptance do not disprove this newly measured accepted document.

## Material proposal

**FUP027-01 — INCORRECT; corroborates B027-03.** Violated obligation: REQ-036 fixed 40px rows, mixed placed/unplaced support, and AC-015 valid grid placement. Exact correction boundary: `layout.ts:28–35` and WyrdView's placement/render geometry at lines 108–130 and 310–315. Observable consequence: a declared noncolliding y coordinate changes with preceding intrinsic content, and the same unchanged later panel jumps when that preceding panel is first edited into a fixed layout. Pointer snapping and rendered coordinates consequently use different row geometry.

Smallest safe outcome: retain the existing WyrdView geometry owner and pure collision resolver, keep desktop track pitch fixed, and supply collision/placement with the occupied grid extent of intrinsic auto content. Auto components must retain their content height; their occupied extent must cover that content on the same grid used by authored panels and edits. Preserve immutable authored documents and do not serialize collision or measurement results as authored layouts. No replacement layout framework, component size input, per-consumer offset correction, public API, persistence or new dependency is warranted. WyrdView already owns actual DOM measurement (`current`) and responsive measurement; extend that owner rather than adding a parallel geometry subsystem. Merely changing CSS to `grid-auto-rows:40px` would clip or scroll formerly content-height panels, so that alone is not a complete correction.

Focused closure proof: reuse real-browser AppBridge or gallery capability with an intrinsic header and intrinsic Chart followed by noncolliding y=4 and colliding placed panels. Check computed fixed track pitch, the fixed panel's expected 208px noncolliding start where content permits, preserved intrinsic content height, collision push-down to the first clear grid position, no overlap, and unchanged input. Recheck converting the auto panel to fixed does not redefine coordinate pitch. Keep existing all-unplaced and narrow behavior checks; no broad journey suite or new harness is needed for this bounded correction.

## Limits

This pass did not re-review other proposed findings, rerun the full task suites, or modify source. Browser proof used the existing built AppBridge artifact plus the matching source geometry; a fresh bundle build was not needed or run. The native panel layout defect is resolved as an evidence question and still requires the mandated independent final validation.
