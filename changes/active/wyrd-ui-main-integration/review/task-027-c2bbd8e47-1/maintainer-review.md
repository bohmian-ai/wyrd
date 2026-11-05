# TASK-027 cumulative maintainer re-review

Result: **FAIL**. FIND-027-1, FIND-027-2 and FIND-027-3 are closed at the reviewed candidate for their diagnosed paths. One additional material dictionary-boundary defect remains in the complete original-task implementation.

## Subject and authority

Repository `/Users/stevenforrester/Documents/GitHub/wyrd-ui-main-integration`; base `a1e9b48d2550bc8e8ae8ef8037ce813e5beeeddb`; immutable candidate `c2bbd8e47e4752b3772ad75acfaf23f3400ac704`, also HEAD at the end of inspection. No tracked source was edited. Only this assigned report was written. No peer discovery report from this attempt was read.

Reviewed the complete TASK-027 cumulative implementation and latest remediation diff, approved specification revision 12 (REQ-030/036, AC-015), original `tasks/TASK-027-view-layout.md`, prior validated FIND ledger, R1 task and evidence, and this attempt's `subject.md` and `verification.md`. Governing references are AGENTS.md, agent rules, reference router, spec-driven development, maintainer style, applicable design/client and doctrine boundaries, TypeScript guidance, Wyrd UI skill/Svelte/testing guidance and Evidence Thread geometry. No `.codegraph/` exists. Unrelated upstream source and TASK-003-owned chart/action/error remediation in the cumulative git range are not attributed to this task; their shared renderer seams were inspected.

## Changed-surface and caller coverage

Paths below are relative to `crates/wyrd/wyrd-server/wyrd-ui` unless prefixed otherwise.

| Changed owner/symbols | Full owner and consumer inspection | Assessment |
|---|---|---|
| `packages/ui/src/lib/schemas.ts`: LayoutSchema, Layout, PanelSchema | Owning schemas module, ViewDocumentSchema, `view.ts::validateView`, renderer parse, generator, generated catalog and layout-contract scenario | Bounds, strict fields, unique panel IDs, component/schema separation and published view shape are clear. IDs remain arbitrary nonempty strings; the dictionary consequence below is real contract behavior. |
| `packages/ui/src/lib/layout.ts`: constants, Placement, rowsFor, overlaps, place, edit, rowsHeight | Entire module; all production imports are the renderer; all changed grid/edit/mobile tests | Pure placement keeps authored state separate from render-only displacement. Measured occupancy and fixed pitch have one geometry owner. Resize/move share snapping and bounds. |
| `packages/ui/src/lib/WyrdView.svelte`: Props, source/loader/content, kept/draft/reset, heights/placements/ordered, boxStyle/current/commit/cancel/key/drag, markup and styles | Entire component, load/start/settle/abort and action siblings, Chart/CardHeader owners, grip/corner interactions, all layout scenarios | Renderer retains layout state and pointer lifetime; loads remain independent of layout-only updates. Ordinary dictionary lookup pretends inherited values are domain Layout/height values: material finding below. |
| `packages/ui/src/lib/view.ts`: MountViewOptions/mountView | Entire module, direct mount test, package export map, generated `dist/view.d.ts` and `dist/WyrdView.svelte.d.ts` | Optional typed callback projects the same contract through Svelte and DOM hosts; no mandatory host feedback API or persistence was introduced. |
| `packages/ui/src/app/main.ts` | Entire bootstrap: tool-result destroy/remount, executor, theme and no-op per-render layout callback | Existing renderer state gives the intended render-local edits; new results dispose them. |
| Gallery `_gallery/+page.svelte` | Complete changed host/fixture declaration and surrounding paired-mode page; WyrdView invocation | Feedback assigns complete returned documents; uses existing fixture executor and theme owners. |
| Repository `examples/ui-app/src/panels.ts`, `routes/view/+page.svelte` | Entire modules and remoteLoader seam | Same authored document and layout callback contract; no backend or catalog component sizing logic added. |
| `src/lib/ui-library.test.ts`: laidOut/panel and five TASK-027 scenarios | Complete schema, mixed/all-unplaced placement, measured occupancy, editing/non-feedback callbacks, pointer cancellation and narrow scenario bodies; adjacent loader/mount regressions | Focused outcome assertions cover the prior defects. Arbitrary accepted IDs colliding with Object.prototype are not covered. Physical sizing/focus claims rely on recorded browser evidence, not jsdom style checks. |
| `packages/ui/scripts/catalog.ts`, generated catalog | Entire generator and published view-schema portion; package scripts and declarations | Layout is source-derived in the view schema; component input schemas remain size-agnostic. Root's fresh codegen/check/build evidence is consistent with this source. |

No new Rust item, Python API, durable API owner, dependency or transport belongs to TASK-027. No separate generated-declaration mismatch was found.

## Existing-owner and machinery assessment

The pre-task renderer already owned panel arrangement and load lifetime. Extending that component, rather than introducing another renderer/store, is sufficient and is what the candidate does. `layout.ts` is a small coherent pure geometry module explicitly required by the task's collision proof; it does not retain a competing copy of state. `current`, `boxStyle`, mobile ordering and pointer snapping all use that geometry.

For intrinsic occupancy, existing Svelte `bind:offsetHeight` supplies native size observation, including content settling and width changes; a second ResizeObserver service or component size props would duplicate it. Native CSS Grid supplies the fixed track pitch; CSS automatic placement alone cannot enforce the approved document-order collision rules. For gesture lifetime, the existing shared `drag` owns capture/listeners and exposes one cleanup to cancellation/reset/unmount. Chart's brush interaction has a distinct range-selection purpose; it is not an existing grid-edit owner to replace or generalize. Keyboard and pointer editing reuse `edit`, and completion reuses `commit`. These changes earn their code without a framework, extra store or dependency.

The existing safe arbitrary-key lookup alternative is already nearby: `loaders.ts:247,274` uses `Object.hasOwn` for catalog/action dictionaries. `placements` already uses a Map. The remaining defect therefore needs a local dictionary-boundary repair, not new machinery or stricter panel IDs.

## Prior finding closure

| Prior ID | Candidate source closure | Proof assessment |
|---|---|---|
| FIND-027-1 | `WyrdView.svelte:150-155` overlays all retained completed layouts via `done`, keeping untouched panel fields/order and excluding placements. New input resets retained state at 103-108. | Editing scenario completes A then B without echo, checks both in callback two, re-applies it and checks unchanged loads/input; root reran exact selector successfully. Closed for the reported sibling-edit omission. The separate arbitrary-ID defect below does not reopen this diagnosed cause. |
| FIND-027-2 | `cancel` calls `release`; `drag` stop removes move/up/cancel/window-Escape listeners and releases capture; reset/unmount reach that owner. Escape no longer merely clears the draft. | Pointer cancellation scenario covers focused/unfocused grip and unfocused corner, later move/up inertness, fresh completion and pointercancel; root reran it successfully. R1 browser evidence corroborates capture/focus. Closed. |
| FIND-027-3 | `bind:offsetHeight` feeds `place(layouts, heights)`; `rowsFor` rounds occupied extent upward; desktop tracks are 40px and auto boxes align to start with intrinsic height. `current` consumes resolved occupancy; callback uses authored edits. | Pure cases cover short/tall/large intrinsic content and collision push-down; DOM case covers measurement propagation. R1 records physical 40px tracks, short-header y4 top 208px, tall-chart push-down, asynchronous settling, width change and mobile stacking. Root's fresh selectors/check/build pass. Closed; browser evidence was not independently rerun here. |

## Material proposed finding

### MNT-027-R2-01 — INCORRECT: accepted panel IDs can resolve inherited properties as Layout values

**Changed location:** `packages/ui/src/lib/WyrdView.svelte:96,107,113,116-120,152-155`; domain input boundary `schemas.ts:198` permits every nonempty panel ID. Sink locations are `layout.ts:37-39` and `WyrdView.svelte:129-142`.

**Governing obligation/principle:** REQ-036/AC-015 require valid authored layouts to render at their positions and whole-document callbacks to preserve untouched panels. Maintainer style requires typed contracts to describe actual accepted values and prevent invalid state from being hidden behind a broad type. `Record<string, Layout>` here does not make an ordinary object's inherited properties Layout values.

**Reachable path:** an agent/host supplies a valid panel with ID `constructor`, `toString` or `__proto__`, with an otherwise ordinary explicit layout. On initial rendering `kept` is `{}`. `kept[panel.id] ?? panel.layout` selects the inherited Object function/prototype rather than the authored Layout. `place` spreads that non-layout into `{auto:false}` with no coordinates. `boxStyle` consequently emits NaN/undefined grid coordinates and height; `current` also reads missing values, so subsequent editing cannot recover ordinary geometry. This occurs without enabling editing, before any callback or adversarial mutation.

There is a sibling manifestation in completion: after editing a normal sibling, `done[panel.id]` is truthy for an untouched reserved-name panel and `commit` replaces that untouched panel's valid layout with an inherited function/object, contradicting its `ViewDocument` return contract. Height lookups use the same ordinary-object assumption and should receive the same own-entry semantics rather than leaving a second producer of invalid measurement values. The source cause is the renderer's arbitrary-ID dictionary boundary, not schema parsing, CSS, the geometry helper or host feedback.

**Small direct probe:** executed through the pinned toolchain against the existing built modules, without adding a harness or modifying source:

```sh
mise exec -- node --input-type=module -e 'import { ViewDocumentSchema } from "./crates/wyrd/wyrd-server/wyrd-ui/packages/ui/dist/schemas.js"; import { place } from "./crates/wyrd/wyrd-server/wyrd-ui/packages/ui/dist/layout.js"; const doc = { version: "wyrd.view/v1", catalogId: "wyrd.ui/v1", range: {last:"24h"}, panels: [{id:"constructor",component:"Chart",input:{},layout:{x:0,y:0,w:8,h:6}}] }; const parsed=ViewDocumentSchema.safeParse(doc); const kept={}; const layouts=parsed.data.panels.map(panel=>kept[panel.id] ?? panel.layout); console.log(JSON.stringify({valid:parsed.success, placement:place(layouts)}));'
```

Exit 0, output `{"valid":true,"placement":[{"auto":false}]}`. Panel input validation is intentionally separate from document geometry; any valid Chart input may replace `{}` without altering this path.

**Concrete maintenance/user cost:** the declared Layout type hides a runtime function/object at a shared placement boundary. Maintainers fixing downstream CSS or a host callback cannot repair the source; otherwise valid documents break solely because of their IDs. Callback feedback can additionally turn a valid document into an invalid one. This is a bounded correctness finding, not a naming preference or hypothetical hardening requirement.

**Smallest testable correction:** keep the current renderer, schema and records, and apply the nearby existing `Object.hasOwn` pattern when resolving retained layouts, completed callback overlays and measurement fallbacks. Only owned entries may override authored layout/zero measurement; preserve all accepted ID strings and the existing computed-property update behavior. Do not reserve/reject new IDs, duplicate validation, add guards to place/CSS/hosts, or introduce a generic map framework. Inspect those dictionary writes with the reads so special keys retain ordinary own-value behavior after edits.

**Focused closure:** extend the existing component layout/edit scenarios using accepted reserved-name IDs. An explicitly placed `constructor` panel must render its authored coordinates without a callback; editing a normal sibling must preserve the untouched reserved-name layout and keep the callback valid under validateView. An unplaced reserved-name panel must resolve finite measured occupancy, and editing that ID must retain normal completion/cancellation behavior. Keep sibling edits, input immutability and no reload assertions. Existing scenario selectors plus owning UI check/build suffice; no new harness or broad server suite is justified.

## Verification and limits

Reviewed root's nine fresh focused selectors, UI/example checks/builds and codegen results at this candidate. No test weakening or dependency addition closes a defect here. R1's existing RED and browser evidence remain historical evidence; rerunning the same known RED solely to reproduce chronology would not add maintainer proof. The direct reserved-ID probe above is the only extra execution in this pass. No browser, Linux build, live Postgres/MCP journey or broad aggregate was rerun.

No style-only finding is promoted. Export return annotations and terse formatting do not independently demonstrate a contract or maintenance failure and do not drive this result.

Skipped implementation and repeat broad verification. Risk: valid reserved-name panel IDs still break rendering/callback layout until MNT-027-R2-01 is corrected.
