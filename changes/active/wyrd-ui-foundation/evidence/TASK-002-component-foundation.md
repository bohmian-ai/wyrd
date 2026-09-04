---
task: TASK-002
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-013, REQ-016, REQ-066, REQ-081, REQ-127, REQ-128, REQ-129, REQ-130, REQ-131, REQ-132, INV-008, INV-009, INV-015, INV-021, INV-022, AC-003, AC-010]
---

# Component foundation evidence

## Component-to-consumer matrix

Every consumer is an accepted mock page id from
`wyrd-ui/brand/renders/product/README.md`. The matrix is machine-readable in
`brand/components.json` (`consumers`) and enforced by
`component-contracts.test.ts` — a catalog component with fewer than two
consumers fails the suite.

| Catalog component | Accepted mock consumers |
|---|---|
| `Panel` | H-02, C-02, O-01, CR-03, S-01 |
| `Button` | CR-02, CR-04, Q-01, H-01 |
| `Badge` | C-01, CR-03, O-09, S-01, E-04 |
| `Chip` | C-01, O-01, O-10, M-01 |
| `Select` | C-01, O-03, O-04, S-01 |
| `Table` | C-01, C-02, O-04, E-04, CR-01 |
| `KpiTile` | H-02, C-09, O-01, E-01 |
| `Disclosure` | C-05, C-14, E-08, M-01 |
| `StateBlock` | C-01, C-03, O-03, CR-01, Q-01 |
| `CodeBlock` | C-05, C-14, Q-01 |
| `ChartPanel` | C-09, C-11, O-03, O-07, S-01 |
| `Line` | O-03, C-09, C-11, S-01 |
| `Bars` | O-07, C-03 |
| `Spark` | C-09, H-02 |

Trusted application code, implemented and deliberately **not** in the catalog
(`catalog: false`, absent from `src/lib/registry.ts`): `Shell`, `Sidebar`,
`Topbar`, `ModeProvider`, and the specified `ProductPill`. They resolve tenant
identity, session state and navigation, so an authored view must never be able
to place, replace or impersonate them (REQ-129, INV-021).

### Components removed rather than shared

Each had at most one real consumer, or none, in the accepted mocks, so per the
task's stop condition it is not shared foundation: `Heatmap`, `Dist`, `Trend`,
`Histo`, `Tree`, `Dropdown`, `Hero`, `Drawer`, `TraceTable`, `Waterfall`,
`SpanPanel`, `EvalPanel`, `DriftPanel`. The trace, eval and drift panels belong
beside their workspace routes (TASK-005, TASK-012, TASK-013) with real
fixtures; `Dropdown` was replaced by a native `Select`; `Trend` and `Histo` are
`ChartPanel` + `Line`/`Bars`. All thirteen were also still painted with the
`--rune-*` tokens that no longer exist in `brand/theme.css`.

## Catalog / implementation / test parity

`registry.test.ts` and `component-contracts.test.ts` assert, against the
sources rather than in prose:

- every built contract has an implementation, and every implementation a
  contract;
- every built catalog contract is registered, and nothing else is;
- chrome is `catalog: false` and unresolvable by name;
- no component references a custom property the generated theme no longer
  defines (this is what caught the dead `--rune-*` set);
- every contracted token exists in `brand/theme.css`;
- rendered and documented variants agree in both directions;
- catalog prop names and types are JSON-safe and semantic, and no catalog
  component declares a callback prop (REQ-130).

Two violations the checks found and that were fixed rather than waived:
`Sidebar` drew a per-item client/server/control kind bar that is not part of
the product navigation, and the `Topbar` environment pill signalled its state
with a coloured dot alone, which INV-008 forbids; it now reads through `Badge`.

## Behavioural evidence

- **Non-colour status (INV-008)** — every `Badge` tone but `neutral` renders an
  `aria-hidden` glyph before its text; `StateBlock` writes its state word out;
  `Line` gives each series a distinct dash *and* marker with a legend that
  mirrors both; thresholds draw a labeled rule plus a positional tick.
- **No fabricated health (INV-009)** — `ChartPanel` shows no latest value in
  the loading, empty, unauthorized or error states, and `partial` draws the
  plot and the partial notice together rather than presenting an incomplete
  series as complete. `Spark` defaults to `neutral`.
- **Keyboard and accessible naming** — `Select` is a native `<select>` bound to
  a visible `<label>`; `Disclosure` is a native `<details>`; `Table` scrolls in
  a `role="region"` with an `aria-label` and `tabindex="0"`; a `Chip`'s remove
  affordance is a link named "Remove filter <key>: <value>".
- **URL-backed state, not callbacks** — chip removal is an href and `Select`
  submits its enclosing form, so filter state stays in the URL and the catalog
  contract stays non-executable.
- **Light/dark equivalence (REQ-081)** — `Line.test.ts` renders the same chart
  under both `ModeProvider` modes and asserts the trees are identical once the
  token-bearing `style` attributes are stripped: only tokens differ.

## Captures

Style guide at `/styleguide`, light and dark side by side, 1600×1100:

- `TASK-002-states-and-metrics-light-dark.jpg` — badges, buttons, chips,
  selects, metric values, table selection, disclosure and all six async /
  authorization states.
- `TASK-002-charts-light-dark.jpg` — `ChartPanel` with unit, latest value,
  freshness, range, source and canonical link; three dash/marker-separated
  series with a labeled threshold and tick; a bar chart; and an unauthorized
  chart that draws no value.
- `TASK-002-narrow-container-and-chrome.jpg` — the same components in a 340px
  container (chips wrap, the table scrolls in its own region, nothing is
  dropped), and the trusted chrome section.

## Verification

```
pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/registry.test.ts                          4 passed
pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/component-contracts.test.ts   18 passed
pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/charts/ChartPanel.test.ts      9 passed
pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test                                    15 files, 63 tests passed
pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check                                   383 files, 0 errors
pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build                                   built
mise run check:tokens                                                              in sync with palette.json
```

`check` crashed before reading a file at the start of this task: every
devDependency is declared `latest`, so typescript resolved to 7.0.2 while
svelte-check 4.7.2 still reaches for the TypeScript 5 sys API. typescript is
now pinned to `^5.9.0`, which restores the lane; the install re-resolved the
other `latest` ranges as a side effect.
