# TASK-002 Bifrost caller errors and live-table subqueries

Spec: `../spec.md` REQ-002, REQ-006, INV-001, AC-001..AC-003.

1. Caller-fault planning failures (unknown column/table/function, type
   mismatch, ambiguous reference) return a 4xx WyrdError carrying the
   planner's message and suggestion, the same for single-table queries,
   joins, and subqueries, over HTTP, SDK, and MCP `bifrost.query`. Today an
   unknown column returns a generic 400 on one table and a generic 500 inside
   a three-way join. Internal failures stay 5xx and opaque. Messages never
   name tables or columns the caller cannot see.
2. Scalar, IN, and EXISTS subqueries over live (uncompacted) tables succeed.
   Today a scalar subquery over a live table fails with
   WYRD_VALA_500_QUERY_EXECUTION_FAILED ("live Scribe fragment open failed:
   peer security or fragment contract rejected"); joins and self-joins work.
   Fix the root cause in the owning path, not per query shape.

Read `architecture/bifrost-design.md` first. Evidence: Bifrost journeys
(Rust, Python, TypeScript, and MCP) reproducing both failures on live data,
negative case for an unauthorized table, and the owning `test:bifrost` lanes.

## Evidence

Diagnosis (base ef03f624a, run with `WYRD_LOG=warn,vala_bifrost_redux=info`):

- Subquery. Symptom: a scalar subquery over a live table returned 500. Trace:
  `Scribe physical follower could not start error=PostResolutionDecode("... ScalarSubqueryExpr can only be deserialized as part of a surrounding ScalarSubqueryExec")`,
  followed by "live Scribe fragment open failed". Cause:
  `LiveScribeExec::handle_child_pushdown_result` copied every non-dynamic
  pushed filter into the signed fragment predicate. That included
  `id = (<scalar subquery>)`, which Scribe cannot decode. Fix site: that same
  leaf (`oracle/live.rs`). Such filters now stay on the leader's FilterExec,
  which already evaluates every pushed filter because the leaf reports them as
  unsupported. IN and EXISTS decorrelate into joins and take the same path.
- Errors. Symptom: an unknown column returned a generic 400 on a single table
  and a 500 inside a join. Cause: `map_query_planning_error` deliberately
  replaced planner text with a fixed detail. When the planner collects more
  than one error (the same bad column referenced twice), it returns
  `DataFusionError::Collection`, which the mapper did not unwrap, so it fell
  through to the 500 path. Fix site: `oracle/mod.rs`. SQL and logical
  planning now unwrap Context, Diagnostic, Shared, and Collection, and return
  `QueryInvalidSql` with the planner's own message. Physical planning keeps
  its fixed detail. Internal and exhaustion errors are unchanged. Planning
  runs only after `authorize_resolved_tables`, so the session holds only
  tables the caller may read. An unauthorized table is refused with
  QueryForbidden before planning starts.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| Caller-fault 4xx with the planner's message: single table, join, subquery (SDKs, MCP) | `oracle/mod.rs` `caller_planning_fault`, `map_sql_planning_error` | Rust `query_bifrost::unknown_column_carries_the_planners_suggestion`; MCP `query::pg_tests::bifrost_query_errors_are_actionable_before_row_execution`; Python `test_unknown_column_carries_the_planners_suggestion[single,join,subquery]`; TS "unknown column in a $shape query"; unit `oracle::tests::datafusion_query_rejections_are_safe_and_actionable` | PASS |
| Internal failures stay 5xx and opaque | `map_datafusion_error` fallthrough | unit test (Internal, Collection(Execution), exhaustion) | PASS |
| No leak of unauthorized table or column names | unchanged pre-planning `authorize_resolved_tables` | Rust/Python/TS `unauthorized_table_refusal_reveals_no_columns` | PASS |
| Scalar, IN, and EXISTS over live tables succeed | `oracle/live.rs` `reads_scalar_subquery` | Rust `query_bifrost::live_table_subqueries_match_their_joins`; MCP `query::pg_tests::live_table_subqueries_answer_like_joins`; Python `test_live_table_subqueries_match_their_joins[scalar,in,exists]`; TS "live table $shape subquery matches its join" | PASS |

Commands (all pass):

- `mise run fmt`, `mise run lints`, `mise run py:format`, `mise run py:lints`, `mise run ts:format`, `mise run ts:lints`, `git diff --check`
- `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::tests::datafusion_query_rejections_are_safe_and_actionable)'`
- `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(/^query_bifrost::/)'` (8/8; after `db:migrate:all:inner`)
- `mise exec -- scripts/postgres/with-test-postgres.sh -- cargo nextest run --locked -p wyrd-mcp --test mcp -P journey --run-ignored=all -E 'test(/^query::pg_tests::/)'` (8/8)
- Python `pytest tests/integration/test_query_bifrost.py` under test Postgres after `mise run py:setup` (12 passed)
- TS `vitest run tests/integration/query-bifrost.test.ts` under test Postgres after `ts:build` and `ts:build:testing` (12 passed)

Limit: base failure was reproduced through the Rust journeys only. The MCP,
Python, and TypeScript journeys exercise the same server path. Non-goals were
not touched, and no unrelated files changed.
