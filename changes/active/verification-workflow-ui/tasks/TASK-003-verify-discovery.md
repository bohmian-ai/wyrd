---
id: TASK-003
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 4
requirements: [REQ-001, REQ-002, REQ-003, REQ-014, AC-001, AC-008, AC-009]
depends_on: [TASK-002, TASK-003A]
---

# All-status discovery and the Verify dashboard

Server input: [TASK-003A](TASK-003A-execution-status-events.md) supplies queued
lifecycle status events through the existing Bifrost table under approved spec
revision 4. UI consumption stays here; the historical revision-3 execution
evidence below is preserved and is not proof of this new input's delivery.

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

This task is against approved spec revision 4, whose scope transfer is
synchronized. Activation requires only this task's named contract/authority
and predecessor outputs. Unrelated backend delivery
or authority amendments do not block it. This packet consumes existing read/action
capabilities, including TASK-003A's status events, and makes no server edits itself; a missing required capability
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

## Execution evidence

The Verify page at `/t/{tenant}/verify` lists every execution in the range from
one `bifrost.query` population (`packages/ui/src/lib/schemas.ts`, `EXECUTIONS`),
with Status and Verdict as separate columns and filters, two linked charts,
counted facets (Status, Verdict, Verifier, Service, Card, Run, Session),
grouping, paging, and a row that opens TASK-002's execution page and returns to
the same list. Status is the newest TASK-003A status row. Each row carries its
execution, subject Card, selected result, binding and Workload identities.

Commands, run from the repository root:

```bash
mise run ui:check
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts src/lib/ui-pages.integration.test.ts
```

`ui:check` passes (74 tests) and both integration files pass (26 tests) at
`5ef2a850a`; the keyboard and phone-width steps added after it pass in the
browser journey at `66c395825`.

| Proof | Test |
|---|---|
| One population for rows, facets and charts; the status and selected-result SQL; the facet list; row identities | `ui-pages.test.ts`: "executions list, facets and charts", "the Verify dashboard reads one population and keeps its investigation in the URL" |
| Real server: a running row matches `get_execution` (Node only); completed and cancelled rows, their subject identity, facets and charts agree on Node and MCP; refusals and another tenant show no rows | `ui-library.integration.test.ts`: "Verify lists executions with and without a recorded result as one authorized population through every host" |
| Real server, Node only: pending, waiting, retrying, timed out and errored rows match `get_execution` | `ui-library.integration.test.ts`: "Verify lists pending work admitted for a pull request…", "…waiting for deferred evidence…", "…retrying for a trace that has not landed…", "Verify lists a timed out and an errored execution…" |
| Browser: filter, group, brush (rows change), page, open a row and return, by mouse and by keyboard, and at phone width; a superseded load never replaces a newer one | `ui-pages.integration.test.ts`: "the Verify dashboard filters, groups, brushes, pages, opens an execution and returns to the same list" |

TDD: the population, status, facet-list and row-identity changes had a failing
unit test first. The real-server journeys were written after the production query.

Comparison with the approved mock (real page captured by the browser journey in
light and dark at 1280 and 390 wide; mock captured by `selfcheck.mjs --shots`).
The WHERE box, zoom, range, Live, both charts and legends, the facet rail and
its order, and the table columns match. Differences:

- Group by is a select on the right; the mock has a row of buttons on the left
  with the total execution count beside it.
- The chart headings do not show the total count.
- The table pages with Newer and Older; the mock shows "Showing 50 of N" with
  "Show 50 more".
- The mock's Failed column and Failed requirement facet are absent (see the
  decision below on requirement filtering; the column was not ruled on).
- The mock's Repository and Pull request facets and groups are absent; they
  belong to TASK-005 and were removed from this task.

User decisions, 2026-11-17:

- The list reads the newest status row only; an execution with none is pending.
- The list filters by failed verdict and by Verifier, never by a specific requirement.
- The list needs tenant-wide Bifrost read; a Verifier-scoped list is out of scope.
- Repository and pull request context on Verify is TASK-005's.

Not proven:

- The five status journeys above run through the Node executor only.
- No real-server test holds a second, non-selected result for one execution,
  or a completed execution whose result has not yet landed; both are covered
  at unit or component level only.
- No unfinished session or drift execution is listed on a real server.
- Touch input and long-content states were not exercised.

Status: `BLOCKED`. Open: the mock differences above, and AC-009's human
acceptance of the page.
