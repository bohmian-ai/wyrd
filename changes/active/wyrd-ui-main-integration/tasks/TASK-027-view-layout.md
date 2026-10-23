# TASK-027 — Placed, resizable view panels

| Field | Value |
|---|---|
| Task | TASK-027 |
| Spec | `SPEC-wyrd-ui-main-integration` revision 12 (approved) |
| Obligations | REQ-036, AC-015; REQ-030 view-document format as revised |
| Status | Complete — accepted by the caller on 2026-10-23 |
| Depends on | TASK-003 (view schema, `WyrdView`, catalog generator, gallery, AppBridge artifact), including its `version`/`catalogId` correction |

## Outcome and Value

A view document places its panels on a 12-column grid, and a reader whose host
opts in can resize and move panels by pointer and keyboard, like a dashboard.
Agents and authors control layout through the document; catalog components stay
size-agnostic, so every host keeps reusing them unchanged (REQ-036, AC-015).

## Owners, Scope, Consumers, and Prohibited Changes

| Capability | Existing owner/symbol | Inspected callers/tests | Missing behavior | Selected extension | New machinery justification |
|---|---|---|---|---|---|
| Document contract | `packages/ui/src/lib/schemas.ts::PanelSchema`, `ViewDocumentSchema` (strict) | `view.ts::validateView`, `WyrdView.svelte`, `scripts/catalog.ts` (publishes `view`), `ui-library.test.ts`, `examples/ui-app/src/panels.ts`, gallery `doc` | No `layout`; bounds unenforced | Add optional `layout {x,y,w,h}` with REQ-036 bounds to `PanelSchema` | None; same schema owner and generated catalog |
| Grid rendering | `WyrdView.svelte` `.panels` auto-fill grid; `CardHeader` panels span all columns | `ui-library.test.ts` TASK-003 view scenarios; gallery View Document; AppBridge artifact `src/app/main.ts` | No placement, collision push-down, overflow containment or narrow stacking | Extend `WyrdView` layout | None; `WyrdView` already owns panel arrangement |
| Size-agnostic fill | `Chart.svelte` plot box tracks width and height | Gallery and harness evidence from TASK-003 rework | Other catalog components not checked inside a fixed-height panel | Panel box gives each component its size; overflow scrolls in the panel | None |
| Layout editing | `WyrdView` props `doc/executor/load/onaction/mode`; `view.ts::mountView`, `MountViewOptions` | Gallery, `src/app/main.ts`, `examples/ui-app/src/routes/view/+page.svelte` | No handles, keyboard editing or layout handler | Optional host layout handler on `WyrdView` and `mountView`; MCP App keeps edits for its render | None; a prop on the existing renderer, not a layout framework or dependency |

Constraints: no catalog component entry gains a size input; layout is never
persisted (REQ-031 and REQ-025 untouched); no `width` field or compatibility
alias; the document passed to the handler is a new value and the input document
is never mutated by rendering or editing; pointer, keyboard and theme behaviour
follow `brand/` tokens and geometry (1px rules, 2px radius, no elevation or
lift). Shipped pages (use case 1) keep their own layout.

Non-goals: adding, removing or duplicating panels; collapsible rows; per-viewer
memory; saving; drag-and-drop libraries or grid-layout dependencies.

## Approach

1. Extend the panel schema with bounded `layout`; regenerate the catalog.
2. Place panels on the 12-column, 40px-row grid; unplaced panels flow full width below earlier ones; resolve collisions by moving later panels down without rewriting the document.
3. Contain each component in its panel box, scrolling overflow inside it.
4. Stack panels in `(y, x)` order below 720 CSS px and disable editing there.
5. Behind an optional host layout handler, add grid-snapped pointer and keyboard resize/move with Escape cancel, emitting the whole updated document per completed change; wire `mountView` and the MCP App's per-render handler.
6. Show placed and editable layouts in the gallery and the example view page.

## Ordered Implementation Scenarios

### Scenario 1 — Layout document contract

**Behavior.** A panel's optional `layout` accepts whole-unit `{x, y, w, h}` within `0 ≤ x ≤ 11`, `1 ≤ w ≤ 12`, `x + w ≤ 12`, `y ≥ 0`, `1 ≤ h ≤ 48`; out-of-range, fractional or partial layouts and any `width` field fail document validation with a problem naming the field. The generated catalog's view schema carries `layout`; component entries are unchanged (REQ-036, AC-015).

**RED.** Add `TASK-027: Layout document contract` to `src/lib/ui-library.test.ts`; it fails because `layout` is rejected by the strict panel schema. Run:
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout document contract$'`

**GREEN.** Add the bounded `layout` to the panel schema and regenerate `catalog.json`. Rerun TASK-003 scenarios 1 and 3.

**REFACTOR.** Keep bounds declared once in the schema owner; no parallel validator.

### Scenario 2 — Grid placement

**Behavior.** Placed panels occupy their grid columns and rows (40px rows); unplaced panels span 12 columns at content height below every earlier panel; colliding panels push later ones down; the supplied document is not changed; overflowing content scrolls within its panel; a document without any `layout` renders in order as before.

**RED.** Add `TASK-027: Grid placement`; it fails because panels render in the auto-fill grid regardless of `layout`. Run:
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Grid placement$'`

**GREEN.** Place panels from their resolved layout in `WyrdView`. Rerun scenario 1 and TASK-003 scenario 3.

**REFACTOR.** Keep collision resolution a pure step separate from rendering so it is testable without layout.

### Scenario 3 — Layout editing

**Behavior.** Without a layout handler no edit affordance renders. With one, the focused move handle moves by arrow keys and resizes by Shift+arrow keys within grid bounds, Escape restores the pre-edit layout, and each completed change calls the handler once with the whole updated document; pointer resize (lower-right corner) and move (handle) snap to the grid with the same results. Editing never mutates the input document and never triggers a panel reload.

**RED.** Add `TASK-027: Layout editing`; it fails because no handler prop or handles exist. Run:
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Layout editing$'`

**GREEN.** Add the optional handler to `WyrdView` and `mountView`, keyboard and pointer editing, and the MCP App's per-render handler. Rerun scenarios 1–2 and TASK-003 scenario 3.

**REFACTOR.** Pointer and keyboard paths share one snapped-change step.

### Scenario 4 — Narrow views

**Behavior.** Below 720 CSS px panels stack full width in `(y, x)` order keeping `h`, and edit affordances are absent even with a handler.

**RED.** Add `TASK-027: Narrow views`; it fails because narrow views keep grid placement and handles. Run:
`mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-027: Narrow views$'`

**GREEN.** Switch to stacked, read-only layout from the rendered view width. Rerun scenarios 1–3.

**REFACTOR.** None beyond keeping the breakpoint in one place.

## Acceptance Criteria

- Valid layouts render at their grid positions; invalid ones fail validation naming the field; `width` is rejected.
- Unplaced and mixed documents render as specified; collisions never overlap and do not rewrite the document.
- Every catalog component fills a resized panel without overdrawing neighbours; charts redraw at the new size.
- Editing exists only with a handler, by pointer and keyboard, snapped, cancellable, emitting the whole updated document once per completed change, without reloads.
- Narrow views stack and disable editing.
- Gallery, example `/view` page and AppBridge artifact show placed and editable layouts in light and dark, desktop and mobile; the MCP App keeps edits only for its render.

## Expected Write Set and Consumer Closure

`packages/ui/src/lib/schemas.ts`, `catalog.json` (generated), `WyrdView.svelte`, `view.ts`, `src/app/main.ts`; `src/lib/ui-library.test.ts`; gallery `+page.svelte` and `ui-fixtures.ts`; `examples/ui-app/src/panels.ts` and `routes/view/+page.svelte`; the package README if it documents the view format.

## Verification and Evidence

- The four exact scenario commands above, plus TASK-003 scenarios 1 and 3:
  `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Validated portable rendering$'` and
  `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts -t '^TASK-003: Dependent loads and safe actions$'`.
- `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check`, `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build`, `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui --filter @wyrd/example-ui-app check`, `mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui --filter @wyrd/example-ui-app build`, and `mise run codegen:check`.
- Pointer resize/move, chart redraw and narrow stacking: recorded real-browser evidence (screenshots with viewport, mode and actions) of the gallery and the built AppBridge artifact. Automated pointer coverage joins TASK-011's browser driver when it lands.
- Human gallery acceptance recorded per AC-011.

## Material Stop Conditions

- A catalog component cannot fill an arbitrary box without a size input.
- The MCP App host cannot keep per-render state, or editing requires a host capability ext-apps lacks.
- Any requirement to persist, share or reload an edited layout.

## Authority Links

`../spec.md` (REQ-030, REQ-036, AC-011, AC-015); `AGENTS.md`;
`agent_docs/index.md`; `TESTING.md`;
`crates/wyrd/wyrd-server/wyrd-ui/brand/DESIGN.md`.

## Implementation Evidence

Commits `d33de26e4`, plus the example header height in the following commit.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Valid layouts render at grid positions; invalid fail naming the field; `width` rejected | `schemas.ts::LayoutSchema`, `PanelSchema.layout`; regenerated `catalog.json` | `TASK-027: Layout document contract`; `mise run codegen:check` | PASS |
| Unplaced/mixed documents as specified; collisions push down without rewriting the document | `layout.ts::place`; `WyrdView.svelte::boxStyle` | `TASK-027: Grid placement`; browser: resized `latency` 8→10 rows pushed later panels down, no overlap | PASS |
| Components fill a resized panel without overdrawing; charts redraw | `.fill` grid cell with `overflow: auto`; Chart box tracks size | Browser: gallery `latency` plot 266→162px on shrink; artifact plot height 374px after grow; overflowing stat scrolls inside its panel | PASS |
| Editing only with a handler, pointer and keyboard, snapped, cancellable, one whole-document report per change, no reloads | `WyrdView.svelte::key`, `drag`, `commit` sharing `layout.ts::edit`; `onlayout` on `WyrdView`/`mountView`; MCP App passes a per-render handler | `TASK-027: Layout editing` (includes host re-passing the edited document without reload); browser: corner drag −2 cols/−2 rows → `w6 h6`, grip drag +3 rows → `y6`, Shift+→×2 → `w8` | PASS |
| Narrow views stack and disable editing | `NARROW` in `layout.ts`; `ordered`, `editable` in `WyrdView` | `TASK-027: Narrow views`; browser 375px: 0 grips, `scrollWidth` 375 in gallery and artifact | PASS |
| Gallery, example `/view`, AppBridge artifact in light/dark, desktop/mobile; MCP App edits per render only | Gallery `doc` state + `onlayout`; `examples/ui-app` page state; `src/app/main.ts` | Playwright 1440 light/dark gallery, 1200 light/dark and 375 dark AppBridge harness screenshots; example `check`/`build` | PASS |

Commands (all pass): the four exact TASK-027 scenario commands; TASK-003
scenarios 1 and 3; `pnpm test` (37 passed); `pnpm check`; `pnpm build`;
`--filter @wyrd/example-ui-app check` and `build`; `mise run codegen:check`;
`git diff --check`.

Defect found and fixed while proving "no reloads": a host re-passing the same
`executor` rebuilt the default loader closure and reloaded every panel. The
loader now derives from the identity of `load ?? executor`.

Non-goals held: no add/remove panels, no persistence, no catalog component size
input, no new dependency. Human gallery acceptance (AC-011): approved by Steven Forrester on 2026-10-20.
