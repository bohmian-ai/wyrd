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
| Selected result identity | latest checkpoint (`event = 'checkpoint'`, `max(sequence)`) of `bifrost.verification.execution_events`: `value.terminal.result_id`; a direct execution writes no event and is complete once its result row is visible | Supplied. Since revision 4 the checkpoint supplies only the selected result; status comes from the row below. |
| Selected result and verdict | `bifrost.verification.results` joined on the recorded `result_id` (the only row of a direct execution) | Supplied. A completed execution whose result row is not yet visible reads as completed with no recorded verdict. |
| Counts and charts | aggregates of the same derived rows under the same binds | Supplied by `bifrost.query`. |
| Queue lifecycle: `pending`, `running`, `retrying`, `waiting`, `completed`, `cancelled`, `timed_out`, `errored` | newest `event = 'status'` row per execution by `occurred_at` in `bifrost.verification.execution_events` (TASK-003A, REQ-015); no row yet means `pending`; equal instants fall to `event_id`, which only makes the choice repeatable | **Supplied at revision 4** (was an external dependency at revision 3). A status row lost with its process leaves the older status standing, as REQ-015 accepts. A direct execution is never queued: `completed` once its result is visible, else `not recorded`. |
| Exact-Verifier restriction before pagination and counts | `bifrost.query` authorizes by tenant and table object (`bifrost_query:read` on all, schema or table); it has no per-Verifier row rule. `verifier:read` scoped to one Verifier governs only the dedicated execution reads. | **External dependency.** A principal without the table grant is denied the whole read; one with it reads every execution row of its tenant, and opening another Verifier's row is then refused by `verifier:read`. A Verifier-scoped discovery population needs a server-enforced row scope; nothing here emulates it with WHERE. |
| Repository, pull request and review-set membership | TASK-005 admission association | **Not delivered.** No column, facet or group is fabricated; they stay proposed. |
| Failed requirement | `bifrost.verification.execution_outputs.output` (Variant holding the result's `requirements` array) | **External dependency.** Read from source only: the failed ids live inside each result body, so a facet needs every selected body unnested per row, which the task excludes. No failed-requirement column, facet or filter ships; it stays proposed until a retained per-requirement outcome column or projection exists. |

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
- UI `Verify lists executions with and without a recorded result as one authorized population through every host`
- PI `the Verify dashboard filters, groups, brushes, pages, opens an execution and returns to the same list`

Commands (from the repository root unless noted):

- C1 `mise exec -- pnpm test src/lib/ui-library.test.ts src/lib/ui-pages.test.ts` in `crates/wyrd/wyrd-server/wyrd-ui`: 43 passed.
- C2 `mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts -t "Authorized Observe across hosts|exact Verifier definition|Verify lists executions"`: 3 passed.
- C3 the same wrapper with `src/lib/ui-pages.integration.test.ts -t "the Verify dashboard filters"`: 1 passed; also `-t "authenticated traces and spans"` and `-t "Verifier Card page draws|observe boards match|execution page keeps its selection"`: passed after their two superseded assertions were updated (below).
- C4 `mise run ui:check` (check, catalog and contract gate, all unit files): exit 0, 74 tests. `mise run codegen:check`: exit 0. `pnpm build`: exit 0. `git diff --check`: clean.

RED then GREEN, per scenario:

- P population loader: RED `TypeError: Cannot read properties of undefined (reading 'table')` (no `executions` signal); GREEN in `855d5a7c9`.
- U executions list: RED `TypeError: Cannot read properties of undefined (reading '1')` (RecordTable had no executions noun, pills, labels or open intent); GREEN in `396b447fa`.
- P Verify dashboard: the first run failed on the missing route module, which is a setup failure and not a behavioural RED; GREEN in `c9ba57dc0`. Two expectations were corrected while going green because the test was wrong, not the page: the bound range is `$1::timestamptz`, and a second value of one column adds a clause as in Observe.

### Revision 4: status rows (TASK-003A consumed at `7ef2c34cd`)

Candidate commits `8c48a0597`, `9b41ed6ae`. The population query now takes a queued execution's status from its newest status row and reads checkpoints only for the selected result.

- P `executions list, facets and charts read one population in bounded shared queries`: RED `expected ' FROM (SELECT i.execution_id …' to contain 'last_value(status ORDER BY occurred_at, event_id) …'`; GREEN in `8c48a0597`.
- U executions list: the `waiting` row and the `running`, `retrying`, `waiting` tones landed together with their assertion; no RED.
- UI and PI (same selectors as above): edited before their first run against the new query. Their first failures were setup, not behavioural REDs (below); both GREEN in `9b41ed6ae`.
- Commands: C1 43 passed; C4 `mise run ui:check` exit 0, 74 tests; `mise run codegen:check` exit 0; `git diff --check` clean; the task's full integration command over both files: 21 of 22 passed; the one failure, `development environment lifecycle`, is diagnosis 3, and that test and `local signup and personal settings` pass after its fix.

Diagnoses of the unexpected failures:

1. Symptom: a completed Run execution listed as `pending`. Evidence: `sdks/wyrd-sdk-ts/testing/wyrd_testing.linux-x64-gnu.node` was built at 15:15, before the first status-row commit `43821f9d6` (19:33). Cause: the test server addon predated TASK-003A and staged no status rows. Fix site: none in code; the prerequisite `mise run ts:build:testing` was rerun.
2. Symptom: the execution held by `holdPublications` listed as `pending` while its own read said `running`. Evidence: `WyrdTestServer::hold_publications` (`crates/wyrd/wyrd-testing/src/server.rs`) claims the queue row through the verification fixture, not the runner. Cause: no production owner made that transition, so no status row exists; a fixture cannot establish this input. Fix site: the two tests. They now submit the hosted `adds-one` Verifier (`fixtures/cards/verification_capacity`) under `occupyHostedMemory`, so a real runner claims it and its step waits for memory. That wait ends at the hosted code timeout (10 s), after which the runner defers and reclaims, so each test reads `running` immediately after `waitForHostedWaiters` and does not rely on it later. `holdPublications` stays for the cancelled execution: the public cancel route records that row.
3. Symptom: PI `development environment lifecycle` timed out waiting for `/t/dev` after Sign in, also when run alone. Evidence (independent read-only diagnosis, given only the command, failure and diff): the Username field read `development` after the fill, was set to `""` by Svelte's `set_value` during hydration of `login/+page@.svelte`, and the click then raised `invalid` on it with no POST. Cause: the login page's Username input rendered a one-way `value={…}`, which hydration re-applies over text typed before it; `required` then blocks the submit. Under `vite dev` hydration lands after the `load` event the test waits for. Not caused by this candidate. Fix site: `src/routes/t/[tenantKey]/login/+page@.svelte`, outside this task's write set: `value` became `defaultValue`, which still restores the username after a refused sign-in (PI `local signup and personal settings`). The same one-way `value` is on `signup/+page@.svelte` and `settings/+page.svelte`; left unchanged, no gate is red for them.

TDD compliance: **VIOLATED** for the two integration scenarios. UI and PI were written after the loaders and page they exercise, so neither has a RED; an unrecorded exploratory query against a real server also preceded UI and was removed before commit. The unit scenarios were test-first except the route-module case above.

Mechanics deviations:

- WHERE on Verify is the read-only projection of the typed clauses, as the mock draws it; the executions signal refuses `where_sql`, because a free predicate over the derived rows is not one of its checked filter columns.
- The page reads its URL state from the server load (`data.state`), like the execution page.
- Verifier names come from one bounded `cards.list` (kind Verifier, 200) on the host; a refused or longer list shows exact UIDs.
- Existing assertions that Verify was unavailable were replaced by the delivered behaviour: P and PI Verifier Card page (now a Results in Verify link), PI traces journey (Verify is a nav link).
- Status and Verdict are pinned by the Verify page's own CSS over the shared table (fixed 116 px Verdict column); the shared component is unchanged.

| ID | Status | Proof |
|---|---|---|
| REQ-001 | **FAIL (open)** | Delivered and proven: layout, range, two linked charts with a neutral `no verdict` series, counted facets, grouping, separate Status and Verdict, combined and incompatible filters, one population across pages (P population test; P dashboard test; UI totals of three facets, two charts and paged rows equal on Node and MCP; PI). Live is proven only as the `live=1` navigation. Revision 4: Status is the recorded queue lifecycle (P SQL; UI `running`, `cancelled`, `completed` on a real server; U `waiting` pill). Open: no failed-requirement filter; no repository or pull request context (TASK-005); WHERE is a read-only projection, not a general predicate; workload, binding and result identity are filters only and cannot be shown as columns; the browser brush asserts the applied range, not changed rows. |
| REQ-002 | PASS | PI: rows open `/verify/executions/{id}` for a completed and a running execution; Back returns to the same URL with the page cursor or filters and a non-zero `window.scrollY`. Range and grouping live in that same URL (P round-trips them into the load); Back with them set was not separately driven in the browser. Results in Verify filters by the exact `verifier_card_uid` (P, PI). U: Verifier, run, session, status and verdict are separate cells and a row opens by its own `execution_id`. |
| REQ-003 | **FAIL (open)** | Proven: only `bifrost.query` operations build rows, counts and charts (UI op set on Node and MCP; built artifact tool calls; P: nine queries and one `cards.list`); the list and facets carry a `LIMIT`, the charts are bounded by the range and bucket only; typed binds; completed rows equal `verification.get_execution` (UI). The selected-result join on the recorded `result_id` is asserted as SQL text only; no test holds a non-selected result row. No server, SDK or wire file is in the diff. Revision 4: rows carry the recorded lifecycle; a `running` and a `cancelled` row equal `verification.get_execution` (UI). Open: the exact-Verifier row scope is not enforced by the query authority, the external dependency recorded above. |
| REQ-014 | PASS | Chart, FacetRail and RecordTable with the existing loaders and catalog (C4 gate). UI: list, facets and both charts through Node and MCP; the list and both charts, as plain `Chart` panels whose totals equal the paged rows, through BFF `ui/load` and the custom app; list, facets and the verdict chart through the built artifact answered by the real MCP server. A superseded slow load does not replace the newer selection (PI). The dashboard's reads are one independent batch and a refused batch is followed by no read (P). |
| AC-001 | **FAIL (open)** | Proven: UI creates failed and passed Run executions, a scheduled drift window, three session turns with their group execution, a runner-claimed execution found by `status = running` with no verdict and then `completed` and `passed` once released, and a cancelled-while-held execution listed as `cancelled`, and asserts rows, facets and charts; PI filters by `running`, opens that execution, returns, then groups, brushes both charts, pages, opens and returns. Open: the unfinished work is one ordinary running execution; `pending`, `retrying` and `waiting` are not produced on a real server here (the TypeScript test server has no control that holds them; TASK-003A's server journey proves those rows, read with a different query shape), and there is no unfinished session or drift execution. No pull request execution exists to list (TASK-005 not delivered). |
| AC-008 | **FAIL (open)** | Proven (UI): another tenant reads empty rows, facets and charts on Node, an empty list on MCP and an empty BFF panel; `verifier:read` alone is denied, not empty, on Node (list, facets, both charts), MCP, BFF and the custom app; an unreachable server is an error on Node, MCP, BFF, the custom app and in the built artifact (there a failing host callback). Open: a Verifier-scoped viewer cannot be expressed (external dependency); result-visibility lag is proven only at component level (U row `exec_lagging`), not produced on a real server; the other tenant holds no executions of its own. |
| AC-009 | **FAIL (open)** | Proven: `mise run dev:full` (config directory redirected to a scratch path so the saved credential was kept; batch passed) needed no setup change: after signing in as `development`, `/t/dev/verify` listed its real inputs, 830 completed executions (780 passed, 50 failed) of `follows-policy 1.1.0` and `no-refund-promise 1.1.0` for `support-agent` / `support-desk`, direct and submitted, 50 rows per page, with both charts and all six facets. Gallery cases added for the executions list, grouped list and facets. Screenshots (local, not committed): `devfull-verify-{light,dark}-{1280,390}.png` from that stack and `execution-verify-dashboard-{light,dark}-{1280,390}.png` from PI. Open: the standard stack holds no unfinished, session, drift or `no verdict` execution; no reference-region comparison with the mock, no keyboard, focus, touch, long-content or error-state review, and no individual human acceptance is recorded. |

An independent read-only check of the task file and diff found a session id shortened like a UUID and the instant sort default applied to authored SQL; both are fixed with their assertions, and the rows above were corrected to what the tests assert. Known and left: the population SQL relies on every read binding the range first (all three callers do); the Verifier chart key names its first five series; the unit REDs cannot be verified from history because each test landed in the commit that made it pass.

Status: `BLOCKED`. Open: REQ-001, REQ-003, AC-001, AC-008, AC-009. Revision 4 closed the queue-lifecycle part of each; `mise run dev:full` and the independent check were not rerun after it. The front-matter status stays `proposed`.
