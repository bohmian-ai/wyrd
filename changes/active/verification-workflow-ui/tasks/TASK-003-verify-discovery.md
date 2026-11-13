---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 3
requirements: [REQ-001, REQ-002, REQ-003, REQ-014, AC-001, AC-008, AC-009]
depends_on: [TASK-002]
---

# All-status discovery and the Verify dashboard

## Outcome

An engineer enters Verify, discovers visible unfinished and completed executions,
combines filters, opens TASK-002's page and returns to the same list state. The
two charts, facet counts and table describe the same authorized population.
Implement shared UI queries/loaders and the page over existing operations; do
not add discovery endpoints, server fields, query services or wire contracts.

## Input, queries and consumer

TASK-002 supplies exact inspection and read adapters. Reuse the existing
`bifrost.query` contract/executors, parameter binding and row/byte bounds to read
retained execution metadata/state plus result summaries. A completed-result table
alone cannot list unfinished work. Use existing accepted identity/status and
selected-result facts; do not replay an execution engine, choose an arbitrary
latest result row or fetch every execution's status/details in the browser.

At activation, record the existing authorized sources for lifecycle, selected
result, filter correlations and counts. Tenant and exact Verifier restrictions
must already be enforced by the selected read/query authority before pagination
and counts. A WHERE clause based on user input is not an authorization boundary.
If current access/data cannot supply an approved field or scoped population,
record that exact external capability dependency and leave the affected work
proposed; do not weaken the boundary or implement a server extension here.

Rows contain exact execution/subject/Verifier, nullable selected result identity/
verdict, lifecycle and existing Service/Workload/Run/session/binding correlations.
TASK-005 supplies actual PR admission association where delivered; no fabricated
membership. Rows/filters/counts/charts use matching bounded query predicates and
the same population across pages. No-result work has a neutral chart bucket,
not a failed or fourth verdict. Status and verdict combine, including empty
incompatible combinations. A live result alone does not establish a retained row.
Do not hydrate full result/detail bodies per row.

Reuse Chart/FacetRail/RecordTable and Observe's investigation grammar at
`/t/[tenantKey]/verify`, with shared portable loaders and existing Node/MCP/BFF
hosts. Match the approved dashboard/mock: dense table including Service, separate
Status/Verdict columns, short legends, general WHERE/range/Live/group controls.
Either chart brushes the shared range. Preserve return position. Rows open the
exact page directly; connect TASK-001's Results in Verify using exact identity.
Grouping is navigation, never a combined judgment.

## Observable proof

Use normal public setup to produce visible queued/running/terminal work,
with and without recorded judgments, enough rows for pagination, distinct tenants
and restricted viewers. Through existing Bifrost/domain read paths and the UI,
assert matching filters/aggregates across pages, no foreign or unauthorized
counts, nullable correlations and truthful denied/unavailable states. Include
result-visibility lag. Record any source/access prerequisite that is absent;
fixtures cannot establish it.

Prove the list load uses bounded shared queries, without per-row details/status
hydration. In the production browser, filter/group/brush/page, open TASK-002's
exact intermediate/terminal execution and return. Preserve filters/range/position;
superseded loads cannot replace new selection. Repeat affected reads through
Node/MCP/custom/built-artifact hosts and compare charts/facets/table/empty/error
states with the mock. Existing read permissions remain unchanged.

## Execution and verification

This task is against approved spec revision 3, whose scope transfer is
synchronized. Activation requires only this task's named contract/authority
and predecessor outputs. Unrelated backend delivery
or authority amendments do not block it. This packet consumes existing read/action
capabilities and adds no underlying server changes; a missing required capability
remains an explicit external dependency. Record the integrated source revisions
and actual dependency journeys it consumes; upstream task status is not proof.

Read the repo-local `wyrd-ui` skill before UI edits. Reuse the shared component,
loader, catalog, Node/MCP executor and host action owners. Test shared components
in `src/lib/ui-library.test.ts` and pages in `src/lib/ui-pages.test.ts`;
authorized host/built-artifact proof belongs to `ui-library.integration.test.ts`
and production BFF/Chromium/public-login proof to `ui-pages.integration.test.ts`.
Record actual focused selectors for added tests before their first run; follow
the existing TDD discipline without inventing test names in this task.

```bash
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.test.ts src/lib/ui-pages.test.ts
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui check
mise exec -- pnpm --dir crates/wyrd/wyrd-server/wyrd-ui build
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts src/lib/ui-pages.integration.test.ts
```

Narrow test files with actual relevant selectors. The integration command needs
the existing `ts:build`, `ts:build:testing`, `build:server` and UI-build
prerequisites; inspect `mise.toml` and the managed Postgres wrapper before
running it. Missing prerequisites are not passing or skipped proof.

Extend normal `mise run dev:full` public setup where needed to demonstrate this
task's real inputs. Independently compare each introduced component/page with
the approved mock on desktop/mobile, light/dark, keyboard/focus/touch, long
content and error states. Record reference regions, screenshots and individual
human acceptance. Fixtures and automated checks alone do not prove fidelity.

Use the existing UI integration owners to prove real Bifrost/domain inputs through
Node/MCP/BFF/custom/built-artifact adapters. Do not add or change server/SDK wire
contracts or their generators. Regenerate the UI catalog when schemas change and
run `mise run codegen:check` where applicable. Full shipped-surface journey lanes
belong to integrated change review; focused UI/host journeys prove this task.
