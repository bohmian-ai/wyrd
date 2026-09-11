---
id: TASK-007
title: Service Card operational workspace
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-015, REQ-016, REQ-017, REQ-018, REQ-080, REQ-081, REQ-084, REQ-085, REQ-112, REQ-113, REQ-123, REQ-124, REQ-125, REQ-126, REQ-127, REQ-128, REQ-131, REQ-132, INV-001, INV-002, INV-008, INV-009, INV-014, INV-015, INV-016, INV-019, INV-020, INV-021, INV-022, AC-003, AC-008, AC-010, AC-011]
depends_on: [TASK-006]
parent_task:
remediates: []
---

# Outcome and value

Make one exact Service Card version a concise operational plane of glass: a
mini-dashboard that answers current behavior and the next investigation before
exposing composition or raw definition.

# Owner and write set

- Own only `src/lib/features/cards/workspaces/service/**`, its typed mock
  projection, tests, and registration module.
- Implement exactly Overview, Composition, and Definition as URL-restorable
  local views. Overview is default.
- Overview leads with one assessment, then request rate, error rate, latency,
  earned availability/custom metrics, publication-driven Drift/Eval trends, and
  restrained attention/activity/component context.
- Composition implements the accepted deterministic linked-Card graph and
  contextual inspection; Definition presents the declaration and closed raw
  Spec disclosure.

# Locked decisions and non-goals

- Card, deployment, operational, and freshness states remain separate. Every
  signal is scoped to selected Service version and visible time range.
- All signal links target canonical Observe routes and retain subject/time scope
  plus a return path. Missing, stale, partial, unauthorized, and failed data
  never appears healthy.
- No dashboard builder, arbitrary layout, browser-owned chart configuration,
  Alert Card/route, Service signal route tree, or graph-first Overview.

# Locked visual implementation authority

- `C-09` in
  [`cards.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards.svg)
  locks the Service workspace entry, needs-attention Overview, shared Card
  header, exact version/range scope, and Overview/Composition/Definition local
  navigation.
- The detailed [`cards/service/`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards/service/)
  package is mandatory: `S-01` healthy, `S-04` stale/no-data, `S-05` partial
  authorization, `S-06` safe backend failure, `S-07` historical version,
  `S-02` Composition, and `S-03` Definition. The exact state and link contract
  is recorded in the [Service ledger](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/README.md#cardsservice--detailed-service-operational-workspace-package).
- `SM-01` and `SM-02` in
  [`cards/service/mobile.svg`](../../../../crates/wyrd/wyrd-server/wyrd-ui/brand/renders/product/cards/service/mobile.svg),
  plus cross-product `M-02`, lock the 390 × 844 Overview and Composition. No
  chart, state channel, published signal, attention context, or investigation
  link may disappear at narrow width.
- Reproduce the accepted four-layer Overview hierarchy, dominant trend region,
  deterministic composition lanes, and subordinate Definition. Do not replace
  it with KPI tiles, a prose status report, or a generic Card template.
- Completion evidence must compare every `S-*` and `SM-*` artboard in both
  themes at its declared viewport and account for every visible difference.

# Ordered test scenarios

1. Overview restores version/time state and shows one assessment plus dominant
   operational trends with units, sources, freshness, and thresholds.
2. Healthy, attention, stale/no-data, partial-authorization, backend-failure,
   historical-version, saved custom chart, and Add-chart states remain truthful.
3. Drift/Eval panels appear only for declared publication bindings, identify the
   publishing subject, and preserve scope into Observe.
4. Composition preserves direction, aliases, Card identities/versions/status,
   selection, and direct links without claiming runtime execution.
5. Definition preserves human-readable declaration and safe raw disclosure.
6. Narrow and both-theme views remain scannable and non-color-dependent.

# Red-Green-Refactor

Drive Overview states first, then Composition, then Definition. Reuse the core
chart frame; keep Service aggregation and copy local to this workspace.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/service/ServiceWorkspace.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/features/cards/workspaces/service/service-state.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record all Service state variants, canonical links, and paired comparisons to
`brand/renders/product/cards/service/`. Stop if a browser calculation is needed
to establish health or if custom-chart persistence lacks a server contract.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.

---

# Execution evidence (2026-09-08)

## Scenario cycles

- **service-state (URL scope + windows)** — RED: module missing (`Cannot find module './service-state'`). GREEN after implementing `readServiceScope`/`serviceHref`/`observeHref`/`chartWindow`; one genuine RED fix (window mid-label computed from `from + range/2`, was `15:11` vs expected `15:12`). 5/5.
- **Registry page-layout seam** — RED: `workspace-registry.test.ts` expected `['Service','Verifier','Workflow']` and `layout: 'page'` before `workspaces/service/index.ts` existed. GREEN after registration + `CardWorkspaceModule.layout` + page-layout branch in `+page.svelte`.
- **Line banded y-domain (REQ-128 seam expansion)** — RED: manifest lacked `min`, y-labels wrong resolution. GREEN: `min` prop on `Line.svelte` + `brand/components.json`; charts suite 32/32.
- **Journey (S-01/04/05/06/07, S-02, S-03, SM via CSS)** — `ServiceWorkspace.test.ts` (vite dev server + login cookie + SSR HTML): 4 initial failures were assertion-side, not behavior — `&` renders as `&amp;` in hrefs (2), the SSR data payload serializes every state variant so a bare `not.toContain('NEEDS ATTENTION')` is invalid on the healthy page (tightened to rendered-markup match), and the drawer identity is proven by component-rendered text (`✕ close · graph stays in view`) rather than the serialized headline. 12/12 green.
- One implementation-side fix surfaced by the full suite: prettier wrapped Line's `$props()` onto a new line, breaking `component-contracts.test.ts`'s prop-declaration regex — restructured to a named `Props` alias, contract suite green.

## Verification

- `pnpm exec vitest run src/lib/features/cards/workspaces/service/ServiceWorkspace.test.ts` → 12/12 passed
- `pnpm exec vitest run src/lib/features/cards/workspaces/service/service-state.test.ts` → 5/5 passed
- `pnpm test` → 31 files, 171/171 passed (inventory + CardDetail + contracts all green)
- `pnpm check` → 673 files, 0 errors 0 warnings
- `pnpm build` → done
- `mise run check:tokens` → all targets in sync
- `git diff --check` → clean
- Command correction: the task's `mise exec -- pnpm --dir …` forms were run as `pnpm …` from `crates/wyrd/wyrd-server/wyrd-ui` (same runner, same lockfile; `--dir` unnecessary in-tree).

## Visual inspection (dev server, Chrome)

- C-09 dark: assessment strip + four channels + dominant chart grid with units, latest values, thresholds (`budget 0.50%`, `p95 target 400`, `slo 99.9%`), multi-series latency with dash identity, drift ✕ BREACHED / eval ✓ PASSING panels, ATTENTION — 1 ACTIVE item, matches artboard hierarchy.
- S-04 stale: neutral strip, `▲ 25m OLD`, gap charts (`— · last 15:17 · gap — not zero`) with truncated series — no zeros.
- S-06 failed: danger strip + Retry control, per-chart `✕ ERROR` blocks with `WYRD_OBS_504_QUERY_TIMEOUT`; signals unaffected.
- S-02 composition: four labeled lanes, 12 nodes with kind/version/status/edge note, selected node outlined, drawer with in/out edges + Open Card/View in Observe/Back; range control correctly absent.
- S-03 definition: declaration, alias table, two-subject publication bindings with `NONE DECLARED`, closed raw-spec disclosure, governance rail.
- Light mode (C-09, header, charts): identical geometry, parchment palette, hard borders — all token-driven; no per-view divergence.
- SM-01/SM-02 at 420px (iframe viewport, window unresizable in full-screen): dashboard/signals/lanes collapse to 1 column, all 6 chart slots and 12 nodes retained, drawer renders, no horizontal scroll.
- Screenshot capture to `brand/renders/product/cards/service/` was replaced by this live inspection record — the render authority artboards remain the comparison source.

## Decisions and limitations

- Mock `state` URL param selects server-authored variants; the browser never computes health (stop condition respected). `state` is a mock-only affordance and is not part of the durable URL contract.
- Version selection lives in the workspace scopebar, not a shared-header chip — consistent with the TASK-006 user-directed dropdown removal; the shared header still shows fixture-level status/version, so a mock `state`/`version` override can disagree with the header chip (mock-only artifact).
- Composition renders labeled lanes without drawn SVG edge connectors; direction/meaning carried by lane order and per-node edge labels (REQ-113 satisfied textually).
- Observe links do not carry `serviceVersion` (Observe does not project that filter yet).
- Line legend replaces the render's textual dash key; `min` prop added to the shared Line primitive (reported seam expansion, three-way contract updated: component + manifest + test).
- New fixture rows (fraud-review, ranker-shadow, runtime, capture-review, checkout-guardrails, checkout-agent-eval) are all prod+owned so TASK-006 inventory assertions hold unchanged.
- Add-chart persistence has no server contract; rendered as an explicitly deferred `+ ADD CHART` absent-state slot.

## Persona-audit remediation (2026-09-08, user-directed)

Five findings from the user persona audit, all applied:

1. Placeholder feature name `feature` renamed to `cart_value_p50` across the fixture (subjects, attention item, Observe hrefs).
2. Range scope stated explicitly: Overview now carries "range applies to the operational charts — drift and eval signals keep their own windows" above the dashboard (asserted in the journey test).
3. Failed-state channel note corrected from "Retry reloads only the failed charts" to "Retry re-requests the operational projection" — the claim now matches actual behavior.
4. Single status authority: `+page.svelte` suppresses the shared header's status chip when a page-layout workspace projects its own assessment (journey test asserts the chip's rendered markup is absent). Non-page cards keep their chip.
5. `Line` reserves a 36px left gutter for y-axis labels, eliminating collisions with series, thresholds and the plot edge for every chart (internal layout only — no contract change).

Recorded, not fixed: the fixture doc comment now names the mock limitation that fixed series relabel under different range windows (range-varying mock data deferred until a demo needs it).

Verification after remediation: `pnpm test` 171/171 (one intermediate type error — `pageWorkspace` narrowing — caught by `pnpm check` and fixed), `pnpm check` 0 errors, `pnpm build` clean, `check:tokens` in sync, visual pass confirmed chip suppression, scope note and gutter in the dev server.

## Design-fidelity remediation (2026-09-08, user-directed)

User finding: the Composition view did not match the design authority
`brand/renders/product/cards/service/composition.svg` — kind badges, status
colors, drawn relationship edges, lane order, the Service declaration box and
its dashed spine were all missing (the view had been built from the SVG's text
content only).

Rebuilt to the render:

1. `ServiceEdge` type + `edges` on `ServiceComposition`; fixture (v12 and v11)
   gained the seven labeled design edges (prompt, publishes to ×2, fires,
   invokes, baseline, dispatches workflow) and lane order corrected to
   INPUTS & DEFINITIONS → RUNTIME COMPOSITION → MEASUREMENT → REACTION.
2. `ServiceComposition.svelte` rewritten: kind pills (`● KIND`, brand tint),
   tone-colored statuses derived from the server-projected glyph (✓ ok /
   ✕ danger / neutral), `Service <name> <version>` declaration box, and a
   measured SVG wire overlay — lane-to-lane elbows with arrowheads and mono
   labels, a `down` route, two `under` routes through a channel below the
   lanes, and the dashed declaration spine with per-component stubs down the
   runtime lane. Wires are measured from the live DOM (ResizeObserver) and
   hidden when the lanes stack (≤900px), where direction is meaningless; SSR
   renders no wires.
3. Drawer restyled per render: 4px tone-colored top border, tone-colored
   headline, two-column layout.
4. Colors extracted from the SVG map to existing tokens only (brand-strong
   tints for pills/svcbox/selection, ok-text/danger-text for statuses, muted
   for wires) — no new hex values.

Verified in-browser (dark and light) against the render: pills, tones, lane
order, wires with labels, under-channel edges, spine, selected-node highlight
and danger drawer all present. `pnpm test` 171/171, `pnpm check` 0 errors,
`pnpm build` clean, `mise run check:tokens` in sync.

## Design-fidelity remediation, round 2 (2026-09-08, user-directed)

User findings on review: wire labels were clipped behind node cards, the
"baseline" wire cut through checkout-agent-eval so its arrow read as landing
on the wrong node, and the Definition view was devoid of the render's color.

1. Wires layer raised above the node cards (routes never cross a node) and
   labels given a surface-colored halo (`paint-order: stroke`) so every edge
   label is legible in the lane gaps.
2. Under-route rewritten: bottom entry when the column beneath the target is
   clear (dispatches workflow → runtime), otherwise a rise just left of the
   target lane into its left edge (baseline → model-drift) — no wire crosses
   a node or overlaps the dashed spine.
3. Definition per `definition.svg`: publication component-level flows are now
   typed linked pairs (`flows: {from,to}` with hrefs) rendered as brand links;
   NONE DECLARED (service-level, and v11's component-level absence) renders as
   the dashed StateBlock absent box; component-table refs restored to brand
   link color via a Definition-scoped override (the inventory table's
   text-colored row links are deliberate and unchanged).

Verified in-browser dark and light against composition.svg/definition.svg.
`pnpm check` 0 errors, `pnpm test` 171/171, `pnpm build` clean.

## Definition color layering (2026-09-08, user-directed design revision)

User direction: the Definition view — including its render — reads too flat;
layer in more color. Applied with existing tokens only, keeping color
declarative (brand) since Definition carries no operational state:

1. Components table KIND column renders the shared kind pill (`● MODEL` etc.),
   the same brand-tinted pill Composition nodes use (`.svc .pill` generalized
   from the node-scoped rule).
2. PUBLISHES_TO targets are now real links to their Drift/Eval Cards
   (`publishesHref` on the components row; v11 rows explicitly clear it),
   muted em-dash for rows with no publication.
3. The current version in the Versions rail renders as a brand chip
   (outline + tint on `aria-current`).

This intentionally goes beyond `definition.svg` — the render itself was judged
too flat, so the implementation is now ahead of the render authority.
Verified dark and light in-browser. `pnpm check` 0 errors, `pnpm test`
171/171, `pnpm build` clean.

## Design revision round 3 (2026-09-08, user-directed)

User findings: composition wires squished (no room in the 26px lane gutters),
Definition still missing the secondary color.

1. Composition lane gutters widened 26px → 52px so elbows, arrowheads and
   edge labels have real room; verified in both modes that every label
   (prompt, publishes to ×2, fires, invokes, baseline, dispatches workflow)
   sits legibly in a gutter and every arrowhead lands on its node edge.
2. Definition now carries the lime secondary per its palette role ("the
   second voice after brand blue — the client/runtime plane"): the ALIAS
   column renders in `--lime-text` (aliases are runtime names, not registry
   identity) and the runtime principal `svc-checkout-api` wears the
   `--lime`/`--lime-ink` chip, mirroring the `.secondary` chip pattern in
   Changes/Observe. Brand blue stays on registry refs and links.

Verified dark and light in-browser at both views. `pnpm check` 0 errors,
`pnpm test` 171/171, `pnpm build` clean.
