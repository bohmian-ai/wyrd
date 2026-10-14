# Independent task invariant review — round 3

## Subject and result

Result: **FAIL**. The new accumulator correctly implements scalar cardinality
for every input it receives, but the shared rewrite can still bypass it using
an unsound logical upper bound. FIND-bifrost-errors-3 is not fully closed.

Immutable Wyrd range: `526355fb8c58dae6d67d516f98dddcd5c02b0e79` →
`dc353ef5ecf39da13f3dcac5b9b5a31558b6a35e`. Pinned DataFusion range:
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` →
`f059acf6c58b2c5fd4bc36b3358e8a56ac283476`. Distributed fork unchanged at
`8fac2a978bdcf8e06457238ebfe9c7f35023c123`. Original task absent; the approved
specification, owner directions, R1 and R2 tasks govern. The full cumulative
runtime/fork diff was reviewed; the latest diff was used to locate R2 changes.

## Governing invariants and navigation

Applicable authority: AGENTS.md, architecture/agent-rules.md,
architecture/references/languages/spec-driven-development.md,
architecture/references/languages/maintainer-style.md,
architecture/references/domain/datafusion.md,
architecture/references/domain/olap-serving.md,
architecture/references/languages/errors.md, and the Bifrost/query ownership
and failure invariants in the Wyrd design/doctrine and Bifrost design.
CodeGraph was used first for the Wyrd live leaf and its consumers. Fork
navigation followed the immutable pinned checkout.

Producer-to-consumer paths inspected:

- `SingleValue::accumulator` → `SingleValueAccumulator::update_batch` /
  `merge_batch` → `state` → final `evaluate`; default function registry and
  normal AggregateUDF codec.
- `ScalarSubqueryToJoin::rewrite` Filter and Projection branches → shared
  `build_join` → `PullUpCorrelatedExpr` → `LogicalPlan::max_rows` → optional
  scalar aggregate → aliased join dependency.
- `AggregateExec::input_distribution_requirements` Final/Single requirement →
  CoalescePartitionsExec → distributed network boundary and Maximum(1).
- Native `execute_scalar_subquery` complete-stream semantics as reference.
- `ScalarSubqueryExpr::snapshot` → `snapshot_physical_expr` → live leaf
  Pre/Post pushdown → first-poll `LiveScribeExec::resolved` → route assignment,
  fragment codec and peer footer accounting.
- SQL logical-planning error mapper, physical-planning mapper and ordinary
  execution-error conversion; compact corpus and remote negative journey.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 caller planning faults retain actionable safe diagnostics | oracle/mod.rs logical/physical mapping split; narrow typed wrapper traversal and resource precedence | mapper unit; Rust/Python/TS/MCP focused journeys retained in cumulative diff and committed evidence | PASS |
| REQ-006 scalar, IN, EXISTS, correlation over live data | existing native/decorrelation owners; common corpus and cross-surface journeys | R1 focused journey evidence and fixed expected results | PASS for covered valid forms |
| R1 ready/pending, typed NULL, metadata, repeated execution snapshot | fork scalar_subquery.rs snapshot; live.rs resolved retains runtime-bound original | fork ready/pending/recursive tests and Wyrd reset/pruning test | PASS |
| R1 analytical remote selectivity and static predicates | two-pass shared session; Post dynamic pushdown; first-poll snapshot | peer scalar journey asserts two fragments/two footer rows and literal-bearing plan | PASS |
| R1 route partitioning neither duplicates nor omits eligible routes | planned routes dealt by partition index before execution-local narrowing | scalar_subquery_resolves_per_execution_after_static_pruning | PASS |
| R1 compact common-query coverage, including CTE join | query_coverage.rs fixed corpus; oversized public workload absent | corpus expected rows over mixed storage; separate remote join | PASS |
| R2 Filter and Projection reject every evaluated multi-row scalar | shared build_join wraps only when max_rows is unknown or >1; FULL JOIN and grouping-set bounds can falsely return 1 | new tests cover direct scans/UNION batches but omit falsely bounded plans | **FAIL** |
| R2 validation global across batches/partitions when wrapped | row counts carried through partial state; Final requires single partition; network coalesce requires Maximum(1) | accumulator merged-state test; slt Partial→Coalesce→Final plan; proto decoded-plan refusal; published/live peer journey | PASS |
| R2 empty, singleton and typed NULL when wrapped | typed-null accumulator initialization; zero rows ignored; singleton copied; return_field preserves metadata | accumulator and slt singleton/empty/type cases | PASS |
| R2 duplicates and NULLs count as rows | update counts array length, merge counts stored row count; rejects before assigning second value | equal/NULL later-row test; duplicate UNION and peer duplicate | PASS |
| R2 no new stage/resource owner or flag reversal | AggregateUDF uses native accumulator/codec; shared query runtime and distributed flag unchanged | fork suite/codec and Wyrd boundary checks in task evidence | PASS |
| INV-001 isolation / diagnostic redaction | immutable tenant-bound cut, routes, fragment binding remain owners; no SQL evaluator or new authority | authorization-redaction journeys and check:tenant-isolation recorded | PASS |
| R2 required Panics documentation / cumulative whitespace | both changed test docs now describe assertion panics; trailing blank removed | source inspection; recorded cumulative diff-check | PASS |
| AC-003 checks for touched surfaces | pins, lock and codec revision advanced together; no second dependency universe introduced | committed R2 command evidence reports all focused gates passing | PASS with execution limits below |

## Finding INV-1 — scalar validation bypassed by false singleton bounds

- Classification: **INCORRECT**; incomplete closure of
  `FIND-bifrost-errors-3` rather than unrelated pre-existing optimizer debt.
- Violated obligation: REQ-006 scalar semantics and R2 criteria 1–3 require
  every evaluated scalar dependency to reject a second row before emitting its
  scalar value.
- Changed location: pinned fork
  `datafusion/optimizer/src/scalar_subquery_to_join.rs:384–385`.
- Invalid-state producer: pinned fork
  `datafusion/expr/src/logical_plan/plan.rs:1457–1468,1487–1493`.

`build_join` trusts `decorrelated_subquery.max_rows() <= 1` to bypass
`single_value`. The bound is documented as a guaranteed upper bound, but two
reachable sibling operators violate that contract:

1. A FULL OUTER JOIN of two singleton inputs uses `left_max * right_max`,
   yielding 1, although disjoint inputs emit both unmatched rows, yielding 2.
2. An Aggregate with nonliteral grouping expressions returns the input bound;
   GROUPING SETS can produce multiple groups from one input row, yielding 2
   while the reported bound remains 1.

Representative projection inputs:

```sql
SELECT (
  SELECT COALESCE(a.x, b.y)
  FROM (VALUES (1)) AS a(x)
  FULL OUTER JOIN (VALUES (2)) AS b(y) ON a.x = b.y
) AS v;

SELECT (
  SELECT COUNT(*)
  FROM (VALUES (1)) AS t(x)
  GROUP BY GROUPING SETS ((x), ())
) AS v;
```

Both inner relations return two rows. The first returns 1 and 2; the second
returns two copies of 1. Both are uncorrelated one-column scalar dependencies.
Under flag-off execution they are passed to unrestricted join consumers
instead of receiving a cardinality assertion. Projection can emit multiple
rows; Filter can accept matches that should have been refused.

### Reachability and sibling trace

- `PullUpCorrelatedExpr::f_down` (decorrelate.rs:135–172) permits uncorrelated
  joins; its default branch preserves the FULL JOIN. Projection reconstruction
  keeps the full join as input, so Projection/Alias forward the false bound.
- `ScalarSubqueryToJoin` applies TopDown (scalar_subquery_to_join.rs:250–251).
  Both outer Filter and Projection invoke `build_join`; the bound is consulted
  when replacing the scalar expression, before later passes can supply a
  cardinality assertion.
- The recommended rule list places scalar rewrite before ExtractEquijoinPredicate,
  EliminateOuterJoin and group elimination (optimizer.rs:299–320).
  Earlier EliminateJoin preserves FULL joins: rewritten_join_type returns
  unchanged for non-Inner joins (eliminate_join.rs:572–573). The projection
  reads both sides through COALESCE and provides no null-rejecting Filter;
  EliminateOuterJoin requires such a Filter and cannot remove this two-row
  shape. Later passes no longer see a ScalarSubquery expression to wrap.
- Aggregate grouping sets are represented explicitly by Expr::GroupingSet
  (plan.rs:4113–4127), so they follow the nonliteral Aggregate branch of
  max_rows. Subsequent conversion to physical group masks does not restore a
  removed scalar contract.
- Other bound producers inspected: Values and EmptyRelation seed known
  cardinalities; Projection, Window, Alias, Repartition and Distinct propagate
  bounds; Filter narrows via functional-dependency uniqueness; Sort/Limit cap
  output; Union sums; Join variants and Aggregate derive bounds. The existing
  second nonrecursive consumer is correlated scalar invariant validation
  (logical_plan/invariants.rs:188). Correcting the shared bound benefits that
  consumer instead of adding a second scalar-only guard.
- Native physical scalar execution reads the complete input and refuses a
  second row (physical-plan/src/scalar_subquery.rs:397–433), irrespective of
  max_rows. This is a path discrepancy introduced into acceptance by relying
  on the bound in R2, not a request to add FULL JOIN or grouping-set support.

### Smallest corrective boundary

Correct `LogicalPlan::max_rows` in the DataFusion fork so FULL JOIN and
grouping-set Aggregate return a conservative upper bound or unknown whenever
they can produce more rows. Reuse that one existing bound owner and keep the
native `single_value` aggregate/codec/global merge unchanged. Preserve the
shortcut for genuinely singleton global aggregates and LIMIT 1, so R1's
runtime-filter performance proof is retained. Do not add Wyrd SQL inspection,
per-consumer FULL JOIN guards, DISTINCT, or LIMIT-based masking.

Focused closure proof: direct bound tests for disjoint singleton FULL JOIN and
singleton grouping sets, then flag-off Filter and Projection SQL tests proving
both scalar shapes refuse a second row; retain empty and valid singleton
controls plus the existing analytical literal/footer proof. Bounds must stay
conservative when arithmetic cannot represent them. No benchmark or general
new qualification framework is needed.

## Prior findings and nonfindings

- FIND-1 remains closed for the accepted selective equality query: native
  HashJoin build finishes before probe polling; the live leaf snapshots its
  full predicate once per execution partition and sends literal expressions.
- FIND-2 remains closed: fixed corpus and focused remote proof cover the owner
  addition without public generators or an embedded DataFusion oracle.
- FIND-3 closes ordinary multi-row scans and cross-partition UNION cases, but
  INV-1 prevents complete closure.
- FIND-4 closes with the two substantive Panics sections.
- `single_value` being a normal SQL-callable aggregate is conventional native
  integration, not a new Wyrd public contract or a duplicate evaluator.
- Existing opaque 500 execution refusal remains safe and outside REQ-002's
  caller **planning** errors; no new finding requests 4xx remapping.
- The accumulator maintains rows in {0,1}, errors on positive excess before
  copying a second value, and merges ordinary trusted native states. No
  reachable state overflow or per-worker-only count defect was found in that
  owner.

## Verification limits

This independent discovery pass was static: no benchmarks, synthetic load,
heavy Cargo reruns or external-server lanes were run. The committed task
commands were inspected as available evidence; green tests do not cover INV-1.
The earlier untraced reservation observation cannot be diagnosed from a later
passing run and is not attributed to this candidate. Integrated capability
verification remains the later change-review gate. The source examples above
are derived from the bound/rule contracts and require focused executable
confirmation during independent validation or remediation.
