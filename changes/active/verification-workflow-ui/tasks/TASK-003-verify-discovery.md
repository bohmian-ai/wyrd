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

## Obligations

Recorded before the first edit. Proof owners are the four existing UI test
files; selectors are recorded under Execution evidence before their first run.

| ID | Promise (required cases) | Contract source | Planned proof |
|---|---|---|---|
| REQ-001 | `/t/[tenantKey]/verify` keeps the approved dense layout: WHERE, range, Live, two compact linked charts (by verdict with a neutral no-recorded-verdict bucket; by Verifier), counted facets, grouping and the execution table. Either chart brushes the range of both. Unfinished, terminal and no-result executions are listed. Verifier, subject/Card, Service/Workload, application Run, session, failed-requirement, status and applicable repository/PR context are kept. Charts, facets and rows read one filtered population across pages. Status and Verdict are separate columns and filters; all filters combine and an incompatible combination is empty. Grouping only navigates. | Outcome; Input ¶3–4; spec REQ-001 | Loader unit test on the shared predicate of rows, facets and both charts; page unit test for brush, grouping and filters; host integration on real unfinished, completed, cancelled and no-verdict executions |
| REQ-002 | A row opens TASK-002's exact page directly. Back restores filters, range, grouping and position. Execution, result, subject, Verifier, binding and application Run stay separate facts. The existing tenant shell and exact Card routing are used; TASK-001's "Results in Verify" opens Verify by the exact Verifier identity. | Input ¶4; spec REQ-002 | Page unit test for URL state; Chromium open-and-return journey |
| REQ-003 | Rows, counts and charts come from existing `bifrost.query` reads with typed binds and row/byte bounds; no endpoint, field, query service or wire change. Rows carry exact execution, Verifier and subject identity, lifecycle, nullable selected result and verdict and the recorded correlations. The selected result is the recorded `result_id`, never the latest result row. No per-row status or details read. Tenant and Verifier restrictions are those of the query authority; a WHERE is never authorization. An absent source is recorded as an external dependency and stays proposed. | Input ¶1–3; spec REQ-003 | Loader unit test on planned requests (one bounded batch, no `verification.*` read); host integration comparing rows with `verification.get_execution` and with a non-selected result row present |
| REQ-014 | The dashboard is built from the existing Chart, FacetRail and RecordTable family, loaders, Node and MCP executors, catalog and BFF dispatch; no second loader or table owner. Superseded loads cannot replace a newer selection. A denied prerequisite causes no dependent read. | Input ¶4; spec REQ-014 | Catalog and contract gate (`brand/gen-theme.test.mjs`); the same loaders through Node, MCP, BFF, custom app and built artifact |
| AC-001 | Create unfinished and completed ordinary, session and drift executions (PR executions where TASK-005 delivers them). Combine status, verdict and general filters, brush both charts, page, group, open a row and return. Counts and rows describe one authorized population; no-result work has no verdict; discovery context survives. | Observable proof ¶1–2 | `ui-library.integration.test.ts` and the Chromium journey in `ui-pages.integration.test.ts` |
| AC-008 | Foreign tenants, restricted viewers, denied prerequisites and unavailable reads through real Node, MCP, BFF, custom and built-artifact adapters: no foreign rows or counts, no follow-on read after a denial, no denial shown as empty. Result-visibility lag is shown as recorded. | Observable proof ¶1–2 | `ui-library.integration.test.ts` refusal and tenancy cases |
| AC-009 | `mise run dev:full` produces this task's real inputs where needed; the page and each changed component are compared with the approved mock in both themes, narrow and wide, keyboard, focus and touch, long content and error states, with screenshots and individual human acceptance. | Execution ¶5 | Gallery entries, recorded screenshots; human acceptance is recorded only when given |

### Sources recorded at activation

Read from current source (`crates/vala/vala-bifrost-redux/src/tables/verification/`,
`crates/wyrd/wyrd-server/src/verification/state.rs`,
`crates/wyrd/wyrd-server/src/components/verification/service/inspection.rs`,
`architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`).

| Need | Existing authorized source | Finding |
|---|---|---|
| Population, one row per accepted execution of every origin | `bifrost.verification.execution_inputs` (`execution_id`, `verifier_card_uid`, `binding_id`, `group_id`, `origin`, `recorded_at`) | Supplied. A result row alone never lists unfinished work. |
| Verifier, subject/Card, Service/Workload, application Run, session, binding | the same row: `verifier_card_uid`, `subject_kind/name/version`, `subject_card_uid`, `service_name`, `service_version`, `deployment_environment`, `workload_card_uid`, `run_id`, `group_id`, `binding_id` | Supplied; each is nullable as recorded. |
| Recorded terminal facts | latest accepted revision (`max(sequence)`) of `bifrost.verification.execution_events`: `value.terminal.result_id` (completed, with the selected result) and `value.failure.status` (`cancelled`, `timed_out`, `errored`); a direct execution writes no event and is complete once its result row is visible | Supplied. |
| Selected result and verdict | `bifrost.verification.results` joined on the recorded `result_id` (the only row of a direct execution) | Supplied. A completed execution whose result row is not yet visible reads as completed with no recorded verdict. |
| Counts and charts | aggregates of the same derived rows under the same binds | Supplied by `bifrost.query`. |
| `pending`, `running`, `retrying`, `waiting` apart | PostgreSQL queue row and approval decisions, served only by per-execution `verification.get_execution` | **External dependency.** No bounded list read supplies it and per-row hydration is excluded. Discovery states the retained fact `unfinished`; the sub-status column and filter values stay proposed until a bounded authorized list projection of queue lifecycle exists. |
| Exact-Verifier restriction before pagination and counts | `bifrost.query` authorizes by tenant and table object (`bifrost_query:read` on all, schema or table); it has no per-Verifier row rule. `verifier:read` scoped to one Verifier governs only the dedicated execution reads. | **External dependency.** A principal without the table grant is denied the whole read; one with it reads every execution row of its tenant, and opening another Verifier's row is then refused by `verifier:read`. A Verifier-scoped discovery population needs a server-enforced row scope; nothing here emulates it with WHERE. |
| Repository, pull request and review-set membership | TASK-005 admission association | **Not delivered.** No column, facet or group is fabricated; they stay proposed. |
| Failed requirement | `bifrost.verification.execution_outputs.output` (Variant holding the result's `requirements`) | To be settled against a real server during implementation: used only if one bounded aggregate supplies it without reading result bodies per row; otherwise recorded here as an external dependency. |

## Execution evidence

Consumed revisions: TASK-001A baseline merge `d118ee49f`; TASK-002 as integrated
at `b05da5597`. Candidate: `b05da5597..HEAD` on
`task/wyrd-ui-main-integration/TASK-005`. Paths below are under
`crates/wyrd/wyrd-server/wyrd-ui/`; `U` is `src/lib/ui-library.test.ts`, `P`
`src/lib/ui-pages.test.ts`, `UI` `src/lib/ui-library.integration.test.ts`, `PI`
`src/lib/ui-pages.integration.test.ts`.

Focused selectors, recorded before each first run:

- P `executions list, facets and charts read one population in bounded shared queries`
- U `an executions list keeps status and verdict apart, names Verifiers the host read, and opens one exact execution`
- P `the Verify dashboard reads one population and keeps its investigation in the URL`
- P `an exact Verifier Card page shows its definition, versions and where it runs` (existing; now expects the Results in Verify link)
