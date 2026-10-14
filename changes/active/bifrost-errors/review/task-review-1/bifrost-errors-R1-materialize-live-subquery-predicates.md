---
id: bifrost-errors-R1
status: ready
spec: changes/active/bifrost-errors/spec.md
---

# Materialize live scalar predicates at the remote-fragment boundary

## Subject and authority

- Approved specification: `changes/active/bifrost-errors/spec.md`, revision 1
- Original task path: none. The specification and the owner's direction in the
  review request are the original task authority.
- Reviewed base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`
- Reviewed candidate: `f1e1be815069b23fa9920a61c1198524ecaad514`
- Reviewed DataFusion revision:
  `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`
- Reviewed `datafusion-distributed` revision:
  `8fac2a978bdcf8e06457238ebfe9c7f35023c123`
- Prior verdict: `changes/active/bifrost-errors/review/task-review-1/verdict.md`
- Validated ledger:
  `changes/active/bifrost-errors/review/task-review-1/findings-validation.md`

Route this task directly to `$wyrd-implement`.

## Issue diagnosis

### FIND-bifrost-errors-1: scalar support causes unbounded live-tail transfer

REQ-006 requires live-table subqueries, and the owner additionally requires a
principled implementation without avoidable query-path regression. DataFusion
populates each `ScalarSubqueryExpr` result before it executes the main input.
The candidate nevertheless detects that concrete expression in
`LiveScribeExec` and removes its entire conjunct before Wyrd builds the Scribe
fragment. DataFusion keeps the leader `FilterExec`, so returned rows are
correct, but Scribe never receives the resolved scalar predicate.

For a selective query such as `id = (SELECT max(id) ...)`, every outer live row
surviving unrelated predicates crosses the remote boundary. Scribe work,
network bytes, Arrow decoding, leader CPU, and exposure to terminal mid-stream
loss scale with the full live tail rather than the qualifying result. The
focused journeys assert row equivalence only and cannot detect this regression.

DataFusion already owns the generic `PhysicalExpr::snapshot` contract for
turning runtime-bound expressions into static expressions before remote
serialization. `ScalarSubqueryExpr` lacks that implementation. Wyrd owns the
private Scribe-fragment boundary and runtime route selection. Those two owners,
used together, close the defect at its source without a concrete-expression
guard or a second evaluator.

### FIND-bifrost-errors-2: oversized analytical qualification machinery

The candidate adds a 687-line public support-workload module, a
published/live/mixed Rust SDK suite, a peer-network suite, registrations, and a
normal DataFusion dependency for `wyrd-testing`. It generates three tables and
hundreds of rows, embeds a second DataFusion session as an oracle, and runs ten
multi-feature analytical queries across four storage/distribution states. The
covered SQL forms are valuable because joins, scalar and correlated
subqueries, membership, CTEs, aggregates, and windows are common data
engineering and data science workflows, and this fix exposed that Bifrost was
missing compact coverage for them.

The defect is the size and ownership of the harness, not the presence of common
query coverage. The suite exposes a reusable public test API and promotes
DataFusion into the normal `wyrd-testing` dependency cone, yet it does not
inspect the Scribe fragment, transferred rows or bytes, or route pruning. It
therefore misses finding 1 while widening compile time and the failure surface.

The existing focused Rust, Python, TypeScript, and MCP journeys cover the
specified SQL forms, planner messages, and authorization redaction. They do not
provide a compact server-side corpus for the wider set of common analytical
query types. Replace the oversized framework with that corpus rather than
deleting the useful coverage goal.

## Intended outcome

A populated scalar-subquery value becomes a normal serializable DataFusion
literal before Wyrd selects live routes and encodes a Scribe fragment. The
resolved scalar predicate and independent static or time predicates execute at
Scribe, while the retained leader filter remains the authoritative correctness
check. Pending scalar state fails as a typed error. Focused journeys continue
to return the same rows across every shipped surface.

The change contains focused proof for error mapping, subquery semantics, and
bounded remote scalar pushdown. The general analytical support-workload
framework is replaced by a compact Rust query-coverage corpus using small fixed
fixtures and explicit expected rows.

## Decision-complete recommendation

1. In the owned DataFusion fork, extend the existing snapshot contract for
   `ScalarSubqueryExpr`. A populated slot snapshots to a metadata-preserving
   built-in literal; an unpopulated slot returns a typed DataFusion error. Use
   the existing recursive physical-expression snapshot mechanism for compound
   predicates. Build the literal with DataFusion's existing `Literal` and
   `FieldMetadata` APIs; do not add a Wyrd-specific expression or serialization
   form.
2. Advance Wyrd's pinned DataFusion revision through the repository's normal
   dependency update path.
3. In Wyrd's live execution path, retain scalar-dependent conjuncts. At the
   start of each `LiveScribeExec::execute` invocation, after
   `ScalarSubqueryExec` has populated its results and before routes are assigned
   to the execution partition or any fragment is opened, recursively snapshot
   the predicate once. Use that same immutable snapshot to refine route
   selection and encode every fragment opened by that invocation. Never
   snapshot or evaluate the scalar per row, batch, or route. Preserve the
   leader `FilterExec`.
4. Delete the concrete scalar-subquery exclusion. Do not replace it with an
   expression allowlist or Wyrd-owned scalar evaluation.
5. Delete the general support-workload module, its generated-data and
   DataFusion-oracle machinery, both broad journey modules, their registrations,
   and the manifest dependency promotion. Retain the existing focused
   cross-surface journeys and the `RunId` fixture support they require.
6. Add a compact Rust Bifrost query-coverage corpus using small hand-written
   fixtures and explicit expected rows. Cover projection/filter/order/limit,
   aggregation with `GROUP BY` and `HAVING`, inner and left joins,
   non-correlated and correlated scalar subqueries, `IN` and `NOT IN` including
   NULL behavior, `EXISTS` and `NOT EXISTS`, and one window function such as
   `RANK`. The CTE case must use a real Bifrost table inside `WITH a AS (...)`
   and join that CTE from the outer query to a second Bifrost table with an
   explicit `ON` or `USING` condition. Exercise the corpus over mixed published
   and live data so it covers Bifrost's composition boundary without
   multiplying every query across every client and storage state.
7. Keep distributed proof narrow: run one representative common join/subquery
   query over remote live data, in addition to the selective scalar-predicate
   fragment test required by finding 1. The remote scalar test must prove
   literal materialization and bounded work; result equivalence alone is not
   sufficient.

This division follows existing ownership: DataFusion converts its runtime
expression state; Wyrd decides when its private remote fragment must become
static. It also keeps the correction independent of the separate ignored
stage-cut issue in `datafusion-distributed`, which is not on Wyrd's Scribe
fragment path.

## Constraints and preserved behavior

- Preserve the reviewed caller-fault mapper and its narrow disclosure boundary.
- Preserve authorization-before-diagnostics and tenant-bound fragment
  assignments.
- Preserve the leader filter as the final row-correctness authority.
- Preserve published and hot-source behavior, dynamic-filter behavior, and
  ordinary/time predicate and route pruning.
- Preserve planning-time pruning from independent static/time conjuncts; an
  unresolved scalar must not make Wyrd discard those conjuncts or defer their
  pruning. Runtime pruning with the resolved full predicate may only narrow the
  already eligible route set, and must occur before partition route assignment.
- Keep the original runtime-bound expression on the reusable physical plan;
  snapshot only the execution-local live leaf so plan reset/re-execution reads
  the new scalar result instead of retaining a stale literal.
- Keep `wyrd-spec` and public error contracts unchanged.
- Do not add a Wyrd scalar evaluator, planner, compatibility path, dependency,
  expression allowlist, or new public API.
- Do not repair or broaden `datafusion-distributed` stage planning in this task.
- Keep common-query coverage test-only and compact; do not add a generator,
  reusable public workload API, embedded DataFusion oracle, or capacity
  framework.
- Do not duplicate the common-query corpus across Python, TypeScript, and MCP;
  their focused smoke journeys continue to prove the shared server contract at
  each public surface.

## Acceptance criteria

1. `FIND-bifrost-errors-1`: DataFusion snapshots a ready
   `ScalarSubqueryExpr` to a metadata-preserving literal and returns a typed
   error for pending state; recursive snapshotting replaces scalar expressions
   inside compound predicates. Coverage includes a typed NULL result and reset
   followed by re-execution with a different scalar, proving no literal is
   retained across executions.
2. `FIND-bifrost-errors-1`: a remote live query with a large outer set and a
   highly selective scalar predicate dispatches a fragment containing the
   materialized literal and no `ScalarSubqueryExpr`, and asserts bounded Scribe
   output rows or transport bytes from the fragment's exact footer/completion
   counters. This is a deterministic functional assertion, not a wall-clock or
   capacity benchmark.
3. `FIND-bifrost-errors-1`: the same focused test includes an independent
   ordinary or time conjunct and proves it remains in the fragment and retains
   planning-time route pruning; the resolved scalar may further narrow routes
   before they are assigned to execution partitions. The test also proves
   partitioning neither duplicates nor omits an eligible route.
4. `FIND-bifrost-errors-1`: existing focused Rust, Python, TypeScript, and MCP
   subquery and error journeys retain their row, error-detail, and redaction
   behavior.
5. `FIND-bifrost-errors-2`: the support-workload symbol, generated-data and
   oracle machinery, its two broad journey modules, and their registrations are
   absent, and DataFusion returns to its prior dev-dependency scope in
   `wyrd-testing`.
6. `FIND-bifrost-errors-2`: one compact Rust corpus with fixed fixtures and
   explicit expected rows covers projection/filter/order/limit, grouped and
   filtered aggregation, inner and left joins, scalar and correlated scalar
   subqueries, NULL-sensitive membership, existence and non-existence, a CTE,
   and a window function over mixed published/live data. The CTE assertion uses
   the shape `WITH a AS (SELECT ... FROM <table_a>) SELECT ... FROM <table_b> b
   INNER JOIN a ON ...` and verifies the expected joined rows.
7. `FIND-bifrost-errors-2`: one representative join/subquery query returns the
   expected rows when its live inputs reside on a remote Scribe; this is
   separate from the scalar fragment and bounded-work assertions in criteria 2
   and 3.
8. The candidate formats cleanly, has no generated contract drift, and passes
   the focused checks below.

## Focused proof

- Run the exact new DataFusion unit tests for ready, pending, recursive, and
  metadata-preserving scalar snapshots, typed NULL, and reset/re-execution using
  that fork's canonical test commands.
- Run the exact new Wyrd remote-live predicate test through `mise exec -- cargo
  nextest run --locked` with its package, test target, and exact test
  expression, under the repository-managed Postgres wrapper when required.
- Run the exact compact common-query corpus test and its representative remote
  live join/subquery test through `mise exec -- cargo nextest run --locked`
  with their package, targets, and exact test expressions, under the
  repository-managed Postgres wrapper when required.
- Run the existing exact Bifrost error-mapper unit test.
- Run `mise run test:bifrost:journey:oracle` for the changed Wyrd remote query
  path.
- Run `mise run check:deps` and `mise run check:tenant-isolation` for the
  dependency and tenant-sensitive boundary.
- Run `mise run fmt`, `mise run lints`, and `mise run codegen:check` for the
  touched Rust and generated surfaces.

No benchmark, synthetic load generator, raised test concurrency, or wall-clock
threshold is part of this remediation. Performance closure comes from plan
shape, route selection, and exact fragment row/byte evidence, so it remains
deterministic and does not interfere with work in other worktrees.

The complete `verify:bifrost` capability gate belongs once in the later change
review, after this focused remediation is complete.


## Implementation evidence

Status: `IMPLEMENTED`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1. Ready scalar snapshots to a metadata-preserving literal; pending errors; recursive; typed NULL; reset/re-execution | `bohmian-ai/datafusion@5697cfa35` on `wyrd-production`: `ScalarSubqueryExpr::snapshot`; workspace pins and `ORACLE_DATAFUSION_REVISION` moved to it (`a4a0b0c76`) | fork `cargo test -p datafusion-physical-expr --lib scalar_subquery`: 10 passed, including the 4 new snapshot tests | PASS |
| 2. Remote live scalar dispatches a literal and no `ScalarSubqueryExpr`; Scribe output bounded by exact footer counters | Interactive path: `LiveScribeExec::resolved` snapshots the scalar. Analytical path: `ResourceShape::apply` sets `max_passes = 2` (`resources.rs`), so the rewritten scalar is a `HashJoinExec` key and its dynamic filter reaches the leaf; the leaf accepts `Post`-phase dynamic filters and snapshots on first poll (`live.rs`, `0bb6957fe`). Test-only Scribe footer-row and last-plan counters (`state.rs`) | `peer_network::live_subquery::remote_scalar_subquery_ships_as_a_literal`: 2 fragments, 2 footer rows, the H2 plan carries the static bound and the resolved instant | PASS |
| 3. Static conjunct keeps planning-time pruning; scalar narrows before dealing; no route duplicated or omitted | `handle_child_pushdown_result` prunes by the conjuncts that snapshot at planning; `resolved(partition)` deals the planned routes by index, then narrows that partition's share | `oracle::live::tests::scalar_subquery_resolves_per_execution_after_static_pruning` (pending → error; H2, then reset to H3; union over partitions equals the narrowed set; literal ships); the remote journey above (H0, H1, H3 never opened) | PASS |
| 4. Existing Rust/Python/TS/MCP subquery and error journeys unchanged | No public contract change | `wyrd-ai` `query_bifrost::*` (9 PASS); `mise run test:bifrost:journey:oracle` (57/57, including MCP) | PASS |
| 5. Support-workload harness, generator/oracle, two journey modules and registrations gone; DataFusion back to a dev-dependency | `32be616d3` | `mise run check:deps` | PASS |
| 6. Compact fixed-fixture corpus over mixed published/live data | `sdks/wyrd-sdk-rust/tests/integration/query_coverage.rs` (`c5016e276`, `d0d9b0eaf`); includes single-row, empty, and NULL scalars | `query_coverage::common_queries_answer_over_published_and_live_rows` | PASS |
| 7. Representative join over live inputs on a remote Scribe | `peer_network/live_subquery.rs` | `peer_network::live_subquery::remote_live_join_returns_its_rows` | PASS |
| 8. fmt, codegen, focused checks | — | `mise run fmt`, `lints`, `codegen:check`, `check:deps`, `check:tenant-isolation`, `git diff --check`: all exit 0; error mapper `oracle::tests::datafusion_query_rejections_are_safe_and_actionable` PASS | PASS |

Commands:

```bash
(cd <datafusion fork, branch wyrd-production> && cargo test -p datafusion-physical-expr --lib scalar_subquery)
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::live::tests::scalar_subquery_resolves_per_execution_after_static_pruning) | test(=oracle::tests::datafusion_query_rejections_are_safe_and_actionable)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::live_subquery::remote_scalar_subquery_ships_as_a_literal) | test(=peer_network::live_subquery::remote_live_join_returns_its_rows)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=query_coverage::common_queries_answer_over_published_and_live_rows) | test(/^query_bifrost::/)'"
mise run test:bifrost:journey:oracle
mise run fmt && mise run lints && mise run codegen:check && mise run check:deps && mise run check:tenant-isolation
```

### Diagnosis: the Analytical path never handed the scalar to the leaf

- **Symptom:** with peers configured, the remote scalar query opened 4
  fragments (pick, H1–H3) instead of 2.
- **Evidence:** the traced physical plan was
  `NestedLoopJoinExec: join_type=Inner, filter=event_time@0 = max(...)@1`, with
  the events union as the build side; the leaf received
  `event_time >= H1 AND true`.
- **Cause:** datafusion-distributed's `with_distributed_planner()` disables
  `enable_physical_uncorrelated_scalar_subquery`, so `ScalarSubqueryToJoin`
  rewrites the scalar into a join. Wyrd's `max_passes = 1` then stopped the
  pushed equality from becoming an equi-key, because `ExtractEquijoinPredicate`
  runs before `PushDownFilter` within a pass. Reproduced in upstream
  DataFusion slt: 1 pass plans the NLJ above; 2 passes plan
  `HashJoinExec mode=CollectLeft` with the scalar as the build side.
- **Fix site:** `ResourceShape::apply`, the one session-config owner for the
  leader, follower, and stage sessions. It now runs 2 passes. The hash join's
  existing dynamic filter then reaches the leaf; `HashJoinStream` polls the
  probe only after its build completes, and the leaf snapshots on first poll.
  No DataFusion or datafusion-distributed change was needed. A
  `HashTableLookupExpr` in a large build's filter encodes as `true`
  (DataFusion proto), so it is safe on the wire.
- **Rejected:** generating a dynamic filter in `NestedLoopJoinExec`. It was
  unnecessary, and the NLJ built the events side, so it would not have reached
  events. Re-enabling physical scalar subqueries in the distributed session
  was also rejected: it would put an unencodable expression into worker
  stages.

### Observed and not reproduced

An earlier `test:bifrost:journey:oracle` run (before `0bb6957fe`) failed
`mcp::pg_tests::agent_runs_three_table_analytical_query_through_mcp`: pod 1
still held 604 bytes of reserved query memory after the query. The run that
followed passed 57/57. That first run had no trace, so no cause is
established. Material risk: a possible race between the query terminal and
the release of its reservation, to diagnose with tracing if it recurs.

Non-goals: the leader `FilterExec` and the join remain the correctness
authority; no Wyrd scalar evaluator or expression allowlist;
datafusion-distributed is untouched; the runtime-bound predicate stays on
the reusable plan; static-conjunct pruning stays at planning; no
benchmarks. No unrelated files changed.
