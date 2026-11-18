---
id: TASK-004B
kind: implementation
status: proposed
spec: SPEC-verification-workflow-ui
spec_revision: 6
requirements: [REQ-001, REQ-002, REQ-003, REQ-014, REQ-018, AC-012, AC-001, AC-008]
supporting_spec: SPEC-wyrd-ui-main-integration
supporting_spec_revision: 28
depends_on: [TASK-004A]
---

# Consolidate and optimize UI queries across all pages and routes

## Outcome, inputs and activation

An engineer can navigate, filter and page throughout the UI without duplicate
reads, repeated analytical work or unnecessary waits for supporting results.
Audit every existing page, route, action and portable loader; optimize evidenced
duplication and record inspected paths that already need no change.
Preserve the exact existing evidence, populations, filters, ordering, judgments,
permissions and visual composition.

Execute only after [TASK-004A](TASK-004A-dev-repository-review.md) finishes. It
supplies the completed integrated revision and existing UI/runtime behavior.
Record that revision before starting; do not reopen or edit its unfinished work.
Use the existing real-server UI fixtures to produce the required evidence data.
No running development session or live GitHub connection is an input here.

**Design and audit:** [UI query consolidation](../design/ui-query-consolidation.md).
**Original SQL:** [catalog and schedules](../design/query-audit/README.md),
[exact SQL/binds/source hashes](../design/query-audit/queries.json).
The SQL catalog covers analytical builders; extend the audit to every route.
Reconcile predecessor changes before recording original requests.
**Verification decisions:** [query verification](../design/query-verification.md).

Preserve the exact approved visual targets: Verify's
[centralized source](../design/mock/src/Verify.svelte) and
[rendered mock](../design/mock/dist/index.html#/verify) govern over its retained
[original canvas](../references/original-canvas/Verify.dc.html). Observe uses
[Traces](../../wyrd-ui-main-integration/references/canvas/Main.dc.html),
[Spans](../../wyrd-ui-main-integration/references/canvas/Spans.dc.html),
[Logs](../../wyrd-ui-main-integration/references/canvas/Logs.dc.html) and
[Metrics](../../wyrd-ui-main-integration/references/canvas/Metrics.dc.html).
Shared stat consumers retain [Home](../../wyrd-ui-main-integration/references/canvas/Home.dc.html),
[Agent](../../wyrd-ui-main-integration/references/canvas/Agent.dc.html) and
[Service](../../wyrd-ui-main-integration/references/canvas/Service.dc.html).

| Page | Audited reads | Selected budget |
|---|---:|---:|
| Traces | 12 | 5: two overview, one facets, list, dependent counts |
| Spans | 11 | 4: two overview, one facets, list |
| Verification | 10 + Card names | 2 + independent Card names |
| Logs | 6 | 3: facets, timeline, list |
| Metrics rail | 3 | 2: facets and series names |
| Selected metric | 6; 7 with labels | 4; 5 with labels; keys on request |

These exclude authentication, optional predicate validation and optional points/
detail reads. Optimize shared StatStrip to one bucket/total aggregate per
compatible population. Keep bounded exact details and authored SQL behavior.

## Existing owners and selected implementation

Work in `crates/wyrd/wyrd-server/wyrd-ui/`:

- Portable `packages/ui/src/lib/{observe,schemas,loaders}.ts`: reuse current
  builders, binding, row/byte limits and component props. Implement the design's
  grouping-set overview/facet/Verification aggregates with typed discriminators
  and per-facet ranking. Keep the Verification relation and separate keyset list.
- Existing Node/MCP executors and supported custom/built-artifact hosts:
  preserve common query planning, stream terminals, AbortSignal behavior and
  connection reuse. No global credential or query-result cache.
- Observe/Verify server loads, render owners and host state: one resolved range,
  one applicable predicate validation, independent primary/supporting results,
  narrow invalidation and coalesced Live refresh. Extend the existing Metrics
  BFF owner for on-demand key discovery, not a Rust/domain API.

Carry all design semantics: child matches select their roots; distinct counts,
null/native facet ordering, independent top-eleven limits and log severity are
unchanged; totals/percentiles are not reconstructed from buckets. Verification
preserves status/checkpoint/result selection, timestamp slack, late results,
nullable verdicts and filter-before-limit. Move its top-three/remaining-series
aggregation into SQL; host code only formats labels. Preserve Metric exactness,
resets/histograms and index/point source separation; currencies never combine.

Use executed-operator diagnostics and measured work to assess consolidation. Combining old SQL
with UNION/CTEs, reducing request count while increasing work, hiding panels or
silently falling back to old query bursts is insufficient. If the selected
approach fails, report measured cause and options before another redesign.

Except for public EXPLAIN/EXPLAIN ANALYZE through the existing query operation,
no backend runtime/storage/authorization change, new domain endpoint/field,
migration, new dependency, evidence projection/cache or UI redesign. No
per-row status/detail hydration or browser-side analytical aggregation.
No benchmark runs, `dev:full`, storage-placement experiments or SLO
requalification are required. Use existing tests and server diagnostics; do not
add engine controls, separate telemetry APIs or a benchmark harness.

Implement public EXPLAIN first under REQ-018/AC-012 and the verification
design. Reuse Oracle/DataFusion, preserve underlying query authorization and
execution selection, and cover shipped query clients. This is the maintainer
authorized exception to the earlier UI-only boundary; no separate endpoint.

## Ordered acceptance scenarios and evidence

Use these entries as the live implementation record: actual assertions,
focused commands/selectors, observed RED/GREEN, original/consolidated SQL,
engine diagnostics and limitations. Preserve already-correct proof rather than
manufacturing RED. The references map once to existing approved behavior;
request budgets implement the maintainer-selected optimization.

0. **Explain real queries through the public API.** From existing SDK/query
   editor/MCP clients, submit EXPLAIN and EXPLAIN ANALYZE with actual bound
   Bifrost SELECTs. Assert plan-only versus real execution, complete plan output,
   unchanged selection/authorization/admission/limits and honest failure.
   Exercise the positive/negative journeys in query verification section 1.
   **RED:** the existing SELECT-only parser refuses EXPLAIN. **Tier:** Oracle
   tests and real-server shipped query journeys. **Authority:** REQ-018/AC-012.
   Evidence (predecessor revision 570c07957):
   - Oracle journey `published::explain_reports_the_selected_plan_with_select_authority`
     (`cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all`
     under `with-test-postgres.sh`). RED: EXPLAIN executed the SELECT and returned
     its rows `[[0],[3],[6],[9]]`. GREEN (8.8s): a bound CTE SELECT explains as
     `logical_plan`/`physical_plan` on the Interactive class without executing; ANALYZE
     returns `Plan with Metrics` with `output_rows=4`; the grouped SELECT's plain
     EXPLAIN shows `DistributedExec` with no follower graph lease advanced, and its
     ANALYZE runs Analytical and advances remote work; a statement that fails at
     execution still explains, and its ANALYZE fails `WYRD_VALA_500_QUERY_EXECUTION_FAILED`;
     VERBOSE, nested EXPLAIN and EXPLAIN DELETE are `WYRD_VALA_400_QUERY_INVALID_SQL`;
     a role without payload authority gets `WYRD_VALA_403_QUERY_FORBIDDEN` for both
     forms; no plan text contains `tenants/` or `.parquet`.
   - Unit: `oracle::tests::query_floor_accepts_one_explained_select`,
     `oracle::tests::explained_plans_redact_object_locators` (`-p vala-bifrost-redux --lib`).
     TDD VIOLATED for the parser and redaction: production code preceded a
     runtime RED (the first unit run failed only to compile).
   - MCP `query::pg_tests::agent_explains_and_analyzes_a_bound_read` (`-p wyrd-mcp --test mcp`);
     Python `test_query_bifrost.py::test_explain_returns_plans_and_analyze_executes_the_select`;
     TypeScript `query-bifrost.test.ts` "explain returns plans and analyze executes the
     select". All GREEN; the first MCP run failed on the test's own column-shape assertion.
   - Docs/contracts: SELECT-only wording updated in the spec, CLI, MCP tool and napi
     binding; `codegen:regen`, `ts:build`, `docs:generate` regenerated outputs.
   - Limitations: Analytical ANALYZE reports leader metrics only (follower operators
     show `metrics=[]`). A plain EXPLAIN terminal reports source completion as
     `complete` although no source was read. Deadline/cancellation for EXPLAIN reuse
     the SELECT's unchanged execution path and were not separately exercised.
   - Commit held by the Wyrd Factory hook (uncommitted revision-6 architecture text).
1. **Characterize every route and its requests.** After predecessor completion,
   enumerate every route and portable loader. Record reads, effective inputs,
   dependencies, shared owner, selected change or no-change rationale, and test
   location. Include Cards, repositories, execution details, settings,
   auth/layout, query editor and static/gallery routes alongside dashboards.
   Capture original populated results in existing real-server fixtures with
   fixed bounds and real identities. Keep original SQL as test-only reference.
   **Tier:** source audit, executor characterization and real-server integration.
   **Authority:** REQ-001/003/014 and existing route contracts. Existing correct
   behavior needs no fabricated RED. Evidence (source audit of all 27 route files):
   - No reads, no change: `_gallery` (dev-only PNG/gate), root layout, static
     `+error`/`+layout` and tenant home, `login/api-key`, `signup`, `reset`
     (one auth POST per action), `query` (one authored `bifrost.query` per run),
     `ui/load` (one `loadPanel`), cancel/download (dependent POST then GET).
   - Analytical owners changed below: Observe layout (facets now 1 read; overview,
     StatStrip still pending scenario 2) and Verify (2 Bifrost reads + Card names).
   - Domain-read duplicates found, not yet changed: the hook and tenant layout both
     read `/auth/browser/session` on every page load; Card detail re-reads the same
     Card 3x (plain), 5x (Verifier), 7x (Prompt overview) and lists versions twice;
     Observe validates the same `where_sql` once per panel (up to 4x); account
     settings awaits `/principals/me` only for an id the session holds;
     repositories index awaits the GitHub connection before an independent listing.
   - Deliberate dependent waits kept: Card header gates panels; execution detail
     status gates invocations; GitHub connection gates declare/integration joins.
2. **Consolidate overview, facets and statistics exactly.** With repeated trace
   IDs, child-only matches, nonmatching roots, null/mixed-type facet values,
   timestamp ties and enough values for independent cutoffs, load the portable
   overview/rail/stat components through Node/MCP. Assert baseline-equivalent
   typed values, order and whole-range totals/percentiles at the selected request
   counts; log severity, root/span, token and currency meanings stay exact.
   Engine diagnostics compare complete original/consolidated bundles; query
   structure alone does not prove fewer physical scans.
   **RED:** current independent builders issue excess reads. **Tier:** loader/
   query unit plus real-server host integration, not query-string assertions
   alone. **Authority:** verification REQ-001/003/014 and original Observe/
   StatStrip contracts. Evidence:
   - `ui-library.test.ts` "a facet rail reads every facet in one ranked grouping-set
     aggregate": RED "expected 1 call, got 2"; GREEN after `facetsQuery` (one
     `GROUP BY GROUPING SETS` with a `grouping()`-derived facet index, per-facet
     `row_number()` by `n DESC, value`, limit 11; typed per-facet value columns;
     log severity max kept only for `severity_text`).
   - Updated counts: Observe planning `2+4+1` -> `1+4+1` reads (`ui-pages.test.ts`).
     The shared fixture answers the grouping-set read from its per-column values.
   - Real server: `ui-library.integration.test.ts` traces/spans facet cases and
     `ui-pages.integration.test.ts` "authenticated traces and spans" and
     "authenticated logs investigation" GREEN (Oracle accepts the SQL; counts unchanged).
   - Trace overview: `ui-library.test.ts` "the trace overview reads matching spans and
     roots once each, bucketed and whole-range": RED "expected length 2 but got 5";
     GREEN with `traceOverviewReads`: a spans read and a roots read, each
     `GROUP BY GROUPING SETS ((at), ())` over one scoped subquery; range totals and
     percentiles come from the `()` set (`whole` = 1), never summed from buckets.
     Charts keep their portable per-chart SQL; `executed` names the answering read.
   - StatStrip: `ui-library.test.ts` "TASK-004: Canonical signal interactions": RED
     (old per-stat API, runtime TypeError); GREEN with one read per population (table,
     time, roots, currency grouping): `traces, spans, errors, duration_p95, calls, cost`
     issue 4 reads, was 12. A filter the population does not share becomes the
     aggregate's `FILTER`; a shared one stays in WHERE. Cost groups
     `(at, currency), (currency)`, so currencies never add.
   - Page budgets now: Traces = facets 1 + overview 2 + list 1 = 4 (budget 5); Logs =
     facets 1 + timeline 1 + list 1 = 3; Spans list/facets/strip <= 4; Metrics rail =
     facets 1 + names 1 = 2. `ui-pages.test.ts` Observe planning `1+2+1`; Logs strip
     `logs, log_errors` 1 read (was 4).
   - Real server: 9 Observe tests across `ui-library.integration.test.ts` and
     `ui-pages.integration.test.ts` (stat strip totals incl. null tokens, empty cost,
     subject scope, empty range; traces/spans/logs/metrics browser journeys) GREEN.
   - `pnpm exec vitest run` 83/83; `pnpm check` 0 errors.
3. **Consolidate Verification discovery without changing state.** Produce direct
   completed/missing-result and queued pending/running/waiting/retrying/
   cancelled/errored/terminal executions, multiple checkpoint revisions and
   later results. Through the actual Verify page/shared loaders, assert the
   combined facet/chart read and separate list agree with original SQL under
   status/verdict/correlation filters, paging and chart brushing. Assert two
   Bifrost requests plus bounded independent Card names, no per-row reads and
   server-ranked top-three/remaining chart counts. Labels retain name/version
   disambiguation and UID fallback. **RED:** ten requests and host numerical
   aggregation. **Tier:** loader, real Bifrost and production-BFF browser proof.
   **Authority:** verification REQ-001–003/014, AC-001. Evidence:
   - `executionDiscoveryQuery`/`loadExecutionDiscovery`: one read over the
     `EXECUTIONS` relation with grouping sets per facet plus `(at, verdict_series)`
     and `(at, verifier)`; window `sum`/`dense_rank`/`max` rank Verifiers by
     full-range totals; a final `GROUP BY` folds Verifiers past the top three into
     one null-keyed series per bucket. One scan; no CTE reuse or self-join.
     Charts keep their own portable SQL (`input.sql`) for Chart panels and Query;
     `executed` records the combined read. Host `ranked()` now only formats labels.
   - `ui-pages.test.ts` "executions list, facets and charts..." and "the Verify
     dashboard...": Verify issues 2 `bifrost.query` + 1 `cards.list` (was 10 + 1);
     rendered facets, chart totals and Verifier names unchanged. TDD VIOLATED: the
     observed RED was the removed `loadExecutionCharts` export, not a count.
   - Real server (`ui-library.integration.test.ts` "Verify lists executions..."):
     discovery facets equal the facet rail's rows; both charts total every listed
     execution; top/more match the distinct Verifier count. The test then fails in
     its hosted-runner step ("1 hosted operations never waited for memory within
     60s; last observed: 0"), reproduced 3x and in `ui-pages.integration.test.ts`
     "the Verify dashboard filters..."; not diagnosed, no query code on that path.
4. **Preserve Metrics while removing eager support reads.** Load real scalar,
   integer-above-2^53, cumulative/delta/reset and histogram data at 1h/24h.
   Assert plots/summaries/labels and bounded outputs match original SQL; one
   facets read and series-name lookup accompany the distinct plot calculations.
   Group keys read only on opening the control; points/detail only on request;
   labels read only plotted separate-series IDs. Reopen/change WHERE/range and
   verify no stale suggestions or whole-root point reads. A point-only predicate
   unavailable to an index read remains a refusal. **RED:** eager key sampling
   and two facet reads. **Tier:** existing metric math integration plus loader/
   real browser coverage. **Authority:** original UI REQ-052–057/060/061,
   AC-039/040. Evidence:
   - `ui-pages.test.ts` "group keys are offered from at most 100 of the metric series
     index rows": RED "expected [ { op: 'bifrost.query' } ] to deeply equal []"
     (`loadMetric` sampled keys); GREEN with `loadMetricKeys`, served by the
     existing `observe/metrics` BFF under `?keys`. A selected metric reads chart +
     histogram (+ labels with `apart`); a failed sample still offers the promoted keys.
   - The picker fetches keys on first focus/input per metric view (`seriesKey`), so a
     changed WHERE/range/by never shows another view's suggestions.
   - Real server: `ui-pages.integration.test.ts` "authenticated metrics investigation"
     asserts no `attributes.route` suggestion before the picker is used, then one after
     typing; GREEN. Metric math, labels and points-on-request assertions unchanged.
   - Not separately exercised here: integer-above-2^53 and reset data (existing metric
     integration fixtures unchanged by this work).
5. **Primary content and navigation do not wait on unrelated work.** Delay a
   supporting aggregate or Card-name response in the existing executor/test
   seam: a successful list/plot renders while it remains pending. Counts and
   support errors remain explicit and cannot erase primary content or become
   false zeros. A malformed/denied predicate issues no dependent reads; one
   applicable validation serves the refresh while standalone loaders remain
   safe. Cursor/sort/columns reload only their consumers; range/filter reload
   all affected sections; drawers preserve the dashboard. Race old/new loads,
   navigate away and trigger Live ticks: obsolete work cancels, late results
   cannot replace newer state, refreshes do not pile up, Back/URL restore state.
   **RED:** host Promise.all waits and broad invalidation/interval behavior.
   **Tier:** component/loader tests plus production-browser journey; no
   synthetic host load. **Authority:** verification REQ-002/009/014; original
   UI revision-23 navigation and approved loading/error states. Evidence (partial):
   - One validation per refresh: `ui-pages.test.ts` "observe signal planning and
     interactions": RED "expected 3 validations to have length 1"; GREEN by memoizing
     `predicateFor` per `LoadContext` (WeakMap, dies with the refresh). Standalone
     loaders with their own context still validate; refused predicates still issue no
     dependent read (same test, 3 refusal cases).
   - Streaming: `ui-pages.test.ts` "a client refresh returns its list while supporting
     reads are still pending": RED "expected ['waiting','waiting'] to deeply equal
     ['returned','returned']"; GREEN. On a data request, Verify and Observe loads return
     the list (and metric series) once read; facets, overview, timeline, discovery charts
     and Card names stream as `support`. The first HTML still awaits every section
     (approved first-response contract). Pages follow `support` through
     `$lib/streamed.svelte.ts`: a superseded load never replaces a newer one, and the
     previous section stays until its successor settles; loading sections say so.
   - Live: "Live refreshes the dashboard one load at a time": RED "expected 1 call,
     got 3" with the old `setInterval`; GREEN when each tick waits for the previous
     `invalidate` (Verify and Observe).
   - Real server: `ui-pages.integration.test.ts` 12/13 GREEN; the Verify journey fails
     before its first page load in hosted-memory setup (see scenario 3).
   - Cancellation: "navigating away cancels the supporting reads a refresh is still
     streaming" (`ui-pages.test.ts`, GREEN on first run; regression, no RED): supporting
     reads carry the request's signal, so aborting the request aborts every pending one
     and both streamed `support` promises reject; the list had already returned.
   - Back scroll under streaming: the Verify journey RED "expected 570 to be 667" (Back
     restored scroll before the streamed charts and rail rendered, so the page was
     shorter); GREEN with streaming off, confirming the cause. SvelteKit restores
     snapshots after scrolling, so a snapshot cannot fix it. Fix: `Streamed` remembers each
     address's last settled sections (module scope, 10 addresses, address includes the
     tenant) and a fresh mount shows them until the newest load settles
     (`streamed.test.ts` "a remounted page shows the last section its address settled";
     written with the fix, no separate RED). Streamed charts, totals and Card names are
     polled in the Verify journey.
   - Back/URL restore under streaming: `ui-pages.integration.test.ts` Observe journeys
     (Open in Query then Back, zoom then Back and reload, Open trace then Back to the same
     log record) GREEN through the production BFF with client navigations streaming.
6. **Authorization, terminal failure and supported hosts remain honest.** Exercise
   foreign IDs/tenants, restricted Verifier access, denied tables, unavailable
   dependencies and failed/incomplete query terminals through real Node/MCP/
   custom/built artifacts and BFF adapters. No unauthorized rows/counts or
   sensitive follow-up appears; a successful independent section survives a
   support failure. Preserve row/byte caps, null/empty/error distinctions and
   credentials in host-only owners. Existing correct security/terminal tests
   are regressions, not fabricated RED. **Tier:** existing real-server UI and
   browser host owners. **Authority:** verification REQ-003/005/009/014,
   AC-008; original supported-host/terminal contracts. Evidence (regressions, no RED):
   - `ui-library.integration.test.ts` 14/15 GREEN on the real server, Node and MCP hosts:
     foreign tenant and foreign trace empty, `cards:read`-only principal denied on every
     Observe component with no dependent read, restricted Verifier and narrow-key Verify
     reads, retained-file/download and cancel ownership, terminal states.
   - `ui-pages.integration.test.ts` 12/13 GREEN through the production BFF: RBAC-denied
     first HTML, malformed range refused before reads, empty range truthful.
   - A support failure keeps the list: streamed sections show their own error
     (`streamed.test.ts`); refused predicates refuse only that refresh's panels.
   - The Verify journeys' hosted-runner setup was red ("1 hosted operations never waited
     for memory within 60s"), as were the SDK capacity suites. Cause: since `02efe81fc`
     hosted work waits only when sampled pressure pauses its workload, and the test hold
     was unattributed, so verification never exceeded its share; a verify racing the
     100 ms sampler also saw no pressure. Fix at the shared owner
     (`Sandbox::occupy_for_test`): attribute the hold to the verification workload and
     sample once on taking it. Rust `verification_capacity::verify_waits_for_hosted_memory_then_passes`
     RED (timeout) then GREEN 3/3. Queued executions under pressure return to the queue
     (wyrd-design "A queued execution refused capacity returns to the queue"), so the UI
     journeys now assert the deferred execution listed unfinished (queued or pending, no
     verdict) instead of waiting for a hosted waiter, then completed once memory frees.
   - Still red outside this task: the SDK `submit_defers_under_hosted_memory_then_passes`
     (expects a hosted waiter for queued work) and `verify_refusals_follow_the_arrival_deadline`
     (expects an immediate 503 under full memory; full memory now pauses to the 504
     deadline) in Rust/Python/TypeScript. Both encode pre-`02efe81fc` semantics; owner:
     github-verifier-pipelines TASK-012.
   - Full command `WYRD_UI_INTEGRATION=1 pnpm test src/lib/ui-library.integration.test.ts
     src/lib/ui-pages.integration.test.ts`: 29/29 GREEN. One earlier run failed
     "development environment lifecycle" at the Cards navigation under host load; it
     passed alone and in the next full run.
7. **Verify execution work using existing engine diagnostics.** In the existing
   real-server integration tests, execute original and consolidated bundles on
   the same populated fixture and fixed inputs. Apply [query verification](../design/query-verification.md)
   for collection, attribution and acceptance. Record requests, complete results,
   EXPLAIN/EXPLAIN ANALYZE plans, Oracle stages, available operator work/spills
   and normal-query bundle elapsed time.
   Compare whole-bundle totals, not mean cost per query. Reject incorrect
   results, duplicate fallback bursts, new refusals and unexplained work/resource
   regressions. State unavailable measurements explicitly; fewer requests do
   not establish a speedup. Existing BFF browser assertions verify loading and
   navigation. **Tier:** real-server integration diagnostics and browser tests.
   **Authority:** REQ-001/003/014, AC-001/008 and existing UI behavior.
   Evidence: `ui-library.integration.test.ts` "consolidated Observe bundles return the
   original results; EXPLAIN ANALYZE compares their work" (GREEN), with
   `WYRD_QUERY_DIAGNOSTICS=<file>`. Original SQL comes from `design/query-audit/queries.json`,
   rebound to the fixture range and a 15-minute bucket; same server, fixture and binds,
   no writes between the pair. Results compared as multisets per chart, total, facet and
   stat, plus independent facts (3 spans, 1 trace, 1 error trace).

   | Bundle | Statements | Scan instances | Scan output rows | Operator compute | Spills | Bundle elapsed |
   |---|---|---|---|---|---|---|
   | Traces overview + 5 facets, original | 10 | 10 | 26 | 2.33 ms | 0 | 98 ms |
   | Traces, consolidated | 3 | 3 | 7 | 2.55 ms | 0 | 36 ms |
   | Logs 4 facets, original | 4 | 4 | 4 | 0.94 ms | 0 | 39 ms |
   | Logs facets, consolidated | 1 | 1 | 1 | 0.74 ms | 0 | 36 ms |
   | StatStrip 10 stats, original | 20 | 20 | 18 | 1.60 ms | 0 | 42 ms |
   | StatStrip, consolidated | 6 | 6 | 5 | 2.31 ms | 0 | 39 ms |

   Scans fall 10->3, 4->1, 20->6 with no new spill or refusal. Summed operator compute
   on this three-span fixture is fixed-cost noise (grouping-set expansion adds a little
   on traces and stats), so no compute or latency gain is claimed; elapsed is one
   normal-query run each. Limits: Verify discovery and Metrics are not in this pair
   (Verify's fixture lives in the hosted-blocked test; Metrics reads were removed, not
   rewritten); Oracle DEBUG profiles and peak memory were not collected.

## Verification and handoff

Use existing `ui-library.test.ts`, `ui-pages.test.ts`,
`ui-library.integration.test.ts` and `ui-pages.integration.test.ts` owners.
Record actual focused selectors before RED; do not invent names in planning.
Prepare their existing `ts:build`, `ts:build:testing`, `build:server` and UI build
prerequisites through mise, then run the narrow relevant tests and complete
`ui:check`. If schemas change, regenerate the UI catalog at its owner and run
`codegen:check`. Preserve supported built-artifact/custom/MCP proof; native
request-count mocks alone do not establish real query equivalence.

```sh
mise run ui:check
mise exec -- scripts/postgres/with-test-postgres.sh -- env WYRD_UI_INTEGRATION=1 pnpm --dir crates/wyrd/wyrd-server/wyrd-ui test src/lib/ui-library.integration.test.ts src/lib/ui-pages.integration.test.ts
```

EXPLAIN extends the backend SQL contract, so run the affected Oracle and client
journeys as well as UI tests. Existing lanes are `test:bifrost:journey:oracle`,
`test:bifrost:journey:mcp`, `test:bifrost:journey:python` and
`test:bifrost:journey:typescript`; use their existing test owners and record
exact focused selectors for new cases before RED. Run `fmt` and `lints` for
changed Rust, and the applicable SDK checks if their adapters change. Do not
run an unrelated aggregate gate or benchmark lane.

Use existing real-server UI integration and production-BFF browser fixtures.
Collect public EXPLAIN/EXPLAIN ANALYZE and optional supporting Oracle profiles
for focused diagnostic tests as described in
query verification. No full development server is an input. Record assertions
and evidence in the scenarios above. Do not edit TASK-004A, switch branches,
change another checkout, stage/commit/push, or claim runtime proof from this plan.

Planning revision (2026-11-18): all existing pages and routes are in audit scope.
Verification uses existing tests and engine diagnostics; benchmark runs,
development-stack startup, storage matrices and SLO requalification are excluded.
Public EXPLAIN is an explicitly requested backend addition.
No implementation or runtime proof ran.
