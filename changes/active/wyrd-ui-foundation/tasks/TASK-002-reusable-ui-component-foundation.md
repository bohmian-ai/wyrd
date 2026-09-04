---
id: TASK-002
title: Reusable UI component foundation
kind: implementation
status: proposed
spec: SPEC-wyrd-ui-foundation
spec_revision: 6
requirements: [REQ-013, REQ-016, REQ-066, REQ-081, REQ-127, REQ-128, REQ-129, REQ-130, REQ-131, REQ-132, INV-008, INV-009, INV-015, INV-021, INV-022, AC-003, AC-010]
depends_on: [TASK-001-R3, TASK-001-R5, TASK-001-R6]
parent_task:
remediates: []
---

# Outcome and value

Build the smallest reusable Svelte component foundation earned by the accepted
mocks so later workspaces share Wyrd behavior and brand without becoming the
same page. Preserve a JSON-safe semantic catalog boundary for future composed
views without building that future renderer.

# Owner and write set

- Own `wyrd-ui/src/lib/components/**`, `src/lib/registry.ts`, focused tests,
  `brand/components.json`, and only directly required style-guide examples.
- Reconcile the existing registry and component catalog with current theme
  tokens; remove obsolete or duplicate variants instead of aliasing them.
- Implement only repeated primitives required by at least two accepted mocks:
  badges/chips, buttons and links, filter/time controls, panels, metric values,
  tables, disclosure, standard async/authorization states, `ChartPanel`, and
  the minimum line/bar/sparkline primitives the mocks actually use.
- Record which components are safe catalog entries and which are trusted
  first-party-only components.

# Locked decisions and non-goals

- Props use domain values, labels, units, bounded enums, and safe links. No
  theme-token names, arbitrary colors/classes/CSS/HTML, callbacks, queries, or
  data-fetch URLs cross the catalog contract.
- App shell, navigation, tenant/auth controls, routes, and data clients stay out
  of the catalog.
- Use CSS, SVG, Svelte 5, and installed packages. No chart library, design-system
  framework, renderer, dashboard grid, schema DSL, plugin API, or new dependency.
- Keep specialized composition in feature workspaces; do not add a universal
  page, Card, table, or mega-chart component.

# Ordered test scenarios

1. Catalog validation rejects undocumented variants, missing implementations,
   obsolete tokens, and unsafe serializable props.
2. Shared status and async states remain understandable without color and expose
   accessible names.
3. Table and filter controls work by keyboard and retain useful narrow-container
   behavior.
4. `ChartPanel` renders units, latest value, freshness, range, threshold, link,
   and every required unavailable/error state without fabricating health.
5. Multi-series charts remain distinguishable without color in both themes.

# Red-Green-Refactor

For each scenario, add the focused failing test, implement only that shared
behavior, rerun prior tests, then consolidate only after a second real consumer
is identified in the accepted mocks.

# Exact verification

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/registry.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/component-contracts.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui exec vitest run src/lib/components/charts/ChartPanel.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise run check:tokens
```

# Evidence and stop conditions

Record a component-to-consumer matrix, catalog/implementation/test parity, and
paired theme plus narrow-container captures. Stop if a component has only one
real consumer, needs executable or data-fetching props, or requires inventing a
durable view contract; keep it local or return that behavior to specification.

# Execution skills

Use `$wyrd-implement` and `wyrd-ui`.
