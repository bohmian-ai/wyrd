# Query consolidation: verification decisions

Scope: TASK-004B audits and optimizes every existing UI page/route, after
TASK-004A. Use existing UI test fixtures, executors and repository Postgres.
No benchmark runs, `dev:full`, storage matrix or new performance harness.

## 1. Add public EXPLAIN first

The maintainer explicitly requested public query planning support on 2026-11-18.
Extend the existing Bifrost query operation, not a debug endpoint or a separate
query service. Support `EXPLAIN SELECT ...` and `EXPLAIN ANALYZE SELECT ...`,
including the existing SELECT grammar (CTEs, joins and bound parameters).

- Plain EXPLAIN returns the optimized logical and physical plans without
  executing the underlying SELECT's data scan. Planning may read metadata.
- EXPLAIN ANALYZE executes the SELECT through its normal production path and
  returns the executed physical plan with available operator timings, rows,
  bytes and spill metrics. It returns plan rows, not the SELECT's result rows.
- Reuse DataFusion's tabular `plan_type`/`plan` representation and the existing
  Arrow stream and success/failure terminal. No new terminal fields or custom
  plan serialization. Plan text is diagnostic, not a stable parser protocol.
- Accept exactly one explanation of one supported read-only SELECT. Reject
  mutations, DDL, multiple statements, nested explanation and unsupported
  options with the existing invalid-SQL error. No arbitrary statement bypass.
- Validate and authorize the underlying query before returning any plan: all
  tables, tenant scope, payload columns, CTE/subquery references and parameters
  use existing owners. Explain wrappers must not hide sensitive references from
  plan visitors. Preserve read audit behavior and existing error identities.
- EXPLAIN ANALYZE must preserve the underlying SELECT's production execution
  selection, admission, governed memory/scratch, deadlines and cancellation.
  Do not classify an expensive query as cheap because its output is plan text.
  Apply stream output limits to diagnostic output; never present truncation or
  failed analysis as a complete successful plan.
- Reuse existing plan formatting, suppressing credentials and private connection/
  object-store locators if they appear. No new cluster-topology/debug surface.
- The existing query editor, SDKs, HTTP/gRPC and MCP query consumers can submit
  the SQL through their normal query operation. No new UI workflow is needed.

Existing owners: `vala-bifrost-redux/src/oracle/{mod,planner}.rs`, logical-plan
payload authorization and execution classification; extend their current
visitors/adapters as needed. Inspect all SELECT-only validation in callers,
SDKs and the query editor; route them through the same accepted SQL contract.
Update owning query documentation and only affected generated surfaces.

Test this before query consolidation: plain EXPLAIN returns both plan kinds and
performs no data execution; ANALYZE reports a completed real scan/aggregate with
operator metrics. Compare its underlying execution selection with the same
SELECT. Exercise bound parameters, CTE/join references, denied table/payload,
foreign tenant, invalid SQL, deadline, cancellation and resource/output refusal.
Use owner unit tests plus real SDK -> WyrdTestServer -> SDK journeys. Cover the
shipped Rust/Python/TypeScript and HTTP/MCP paths through their existing query
journey owners; keep actual focused selectors in task evidence. Do not build a
second engine or bypass production admission to make tests pass.

## 2. Record the complete UI request graph

For every route/action and portable loader, record trigger -> actual query or
domain read -> dependencies -> consumer. Include initial load, filters, paging,
sort, details and refresh where present. Record exact effective arguments and
whether independent results are incorrectly awaited together. Mark a selected
optimization or inspected/no-change rationale and its existing test owner.
Count underlying requests, not wrapper functions. Keep auth and deliberate
validation/name lookups explicit rather than hiding them in a SQL budget.

## 3. Prove results before interpreting speed

Use populated existing integration fixtures and fixed absolute bounds/IDs.
Keep original changed SQL as test-only reference, separate from production.
Execute original and consolidated statements through the same public executor;
compare final component props as well as complete typed query results.

Assert exact counts, identities, nulls, large integers, timestamps, filters and
page boundaries. Compare order only where SQL guarantees it; compare unordered
rows as multisets (duplicates matter), preserving the existing tie/cut contract.
Use expected fixture facts as an independent check so two wrong queries cannot
pass merely by agreeing. Include the edge cases in TASK-004B scenarios 2–6.
A failed/incomplete terminal never produces successful partial props.

## 4. Assert consolidation and loading behavior deterministically

Instrument the existing executor/SDK test seam, not a new production monitor.
Assert the selected default page budgets: Traces 5, Spans 4, Verify 2 plus its
independent bounded Card names, Logs 3, Metrics rail 2, selected metric 4/5
without/with labels. Count requested optional validation separately. Verify
no per-row hydration, old-query fallback bursts or eager inspector/key reads.
For other routes assert the specific audited duplicate is removed, rather than
inventing a global request ceiling. Never deduplicate writes or security checks.

Use controlled pending responses in existing UI tests: primary content renders
while support waits, independent failure retains successful content, stale
responses cannot replace current state, and superseded queries cancel. Check
narrow invalidation, Back/URL state and coalesced refresh without stopwatch SLOs.
Use existing production-BFF browser fixtures for the delivered page path.

## 5. Measure actual plans and work during integration tests

For each changed analytical bundle, retain original/consolidated SQL and binds,
plain EXPLAIN and EXPLAIN ANALYZE output, normal SELECT results and actual
request counts. Use the same server build, fixture and fixed inputs; perform no
writes between the pair. Record data identity/cardinality and ordering of the
pair. If warmup, concurrent traffic or a storage transition confounds the result,
do not credit the difference to SQL; repeat the affected pair when comparable.
This is focused query diagnostic evidence in existing tests, not a benchmark
suite or workload generator.

Inspect plans for scan instances, projection/filter pushdown, repeated joins,
aggregate expansion, sorts and bounded output. Compare work summed across the
entire original bundle with the entire new bundle, not cost per query. Keep
independent populations separate. Record compute/row/byte/spill metrics actually
available on executed operators; missing means unavailable, not zero. Scan
output rows are rows emitted by that operator, not necessarily all source rows
visited. Estimated bytes are not measured bytes. Operator time summed across
partitions is not query wall time; do not add parent and child time as CPU cost.

Record normal query submission-to-terminal elapsed time and bundle-start to
last-terminal elapsed time with the existing test executor clock. ANALYZE adds
diagnostic overhead, so do not use its duration as normal page latency. Normal
page concurrency remains part of request/loading tests; diagnostic plan queries
run separately and are excluded from production request budgets.

Existing Oracle profiles supplement these plans: `Oracle.cut_read`,
`cut_materialize`, `scribe_listing`, `provider_setup`, `physical_planning`,
`scan_sizing`, `admission` and operator events. DEBUG `Oracle query profile`
records are emitted after the stream and descendants settle. The test server
already installs telemetry honoring `WYRD_LOG`; enable
`WYRD_LOG=vala_bifrost_redux=debug` for the focused diagnostic test. Attribute
only logs within an isolated query/test span; uncorrelated process-global
counter deltas cannot prove per-query work. Same-name operator profile events
are merged; use EXPLAIN ANALYZE's operator tree for scan-instance claims.
The public stream terminal does not itself carry these profiles.

## 6. Acceptance and handoff

A route passes when results and authority remain correct, its selected duplicate
work is removed, its request assertions pass, and changed analytical SQL has
reviewed plans/metrics showing the intended work reduction. New spills, refusals
or unexplained added scans block acceptance until resolved or the proposed SQL
is rejected. No fallback to old bursts to conceal failure.

Record normal elapsed times as supporting evidence; do not introduce brittle
CI wall-clock thresholds or claim a speedup from noise. If work improves but
latency does not, identify the remaining stage and report the precise outcome.
Unavailable peak-memory metrics do not support a claim of unchanged peak RSS;
record that limit. If the selected approach fails, stop and report the cause
and options before another redesign.

Evidence lives in TASK-004B's scenarios: route/test locations, exact commands,
RED/GREEN, original/new counts, plan artifacts, aggregate work, elapsed times,
and limitations. No separate report framework. Finish relevant UI checks and
existing backend/SDK lanes affected by EXPLAIN. The task is proposed; this
planning document claims no executed tests or measured gains.
