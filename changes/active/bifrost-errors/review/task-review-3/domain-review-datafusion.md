# DataFusion, OLAP, and query-concurrency domain review

## Immutable subject and authority

Wyrd base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`, candidate
`dc353ef5ecf39da13f3dcac5b9b5a31558b6a35e`; DataFusion original
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`, candidate
`f059acf6c58b2c5fd4bc36b3358e8a56ac283476`; distributed fork unchanged at
`8fac2a978bdcf8e06457238ebfe9c7f35023c123`. The candidate HEAD remained
unchanged. Only this assigned report was added.

Authority: approved REQ-002/REQ-006, INV-001, AC-001/AC-003; owner directions
requiring compact common-query/CTE-join coverage and deterministic performance
proof without benchmarks; R1/R2 and prior validation ledgers; AGENTS,
agent-rules, specification workflow, maintainer style, and applicable
DataFusion/OLAP/errors/Rust reference guidance under Bifrost design. CodeGraph
was consulted before locating Wyrd owners; fork inspection expanded directly
into native logical, aggregate, physical, and distributed owners.

## Coverage and positive assessment

| Boundary | Source evidence | Assessment |
|---|---|---|
| Relational scalar producer | `optimizer/src/scalar_subquery_to_join.rs:95-244,363-471` | Filter and Projection share `build_join`; the new native aggregate is the appropriate producer boundary, with one unsafe cardinality shortcut below |
| Native scalar reference | `physical-plan/src/scalar_subquery.rs:397-435` | Zero rows produce typed NULL, a second row fails even across batches, equal/NULL values count as rows |
| Aggregate state | `functions-aggregate/src/single_value.rs:90-192,196-256` | Counts rows, not distinct/non-null values; update and merge preserve the zero/one/multiple-row contract when the aggregate is inserted |
| Type, metadata, memory | `single_value.rs:90-118,185-192` | Output keeps argument type/metadata, nullable output represents empty input, native accumulator size includes retained scalar allocation |
| Partitions/network | `physical-plan/src/aggregates/mod.rs:2062-2078`; distributed `inject_network_boundaries.rs:364-398` | No-group Final requires a single partition; existing network coalesce has Maximum(1); final merge counts rows across partial states rather than checking workers independently |
| Native codec and registry | `functions-aggregate/src/lib.rs:88,142,193`; `proto/src/physical_plan/to_proto.rs:45-80`; `proto/tests/cases/plans/scalar_subquery.rs:282-317` | Ordinary AggregateUDF name encoding and default registration preserve the aggregate; roundtrip regression exercises refusal after decoding |
| Optimizer shortcut authority | `expr/src/logical_plan/plan.rs:1446-1535`, `Filter::is_scalar:2902-2954`, `logical_plan/invariants.rs:172-199` | New rewrite depends on a native upper-bound producer already shared with correlated validation; FULL JOIN upper bound is unsound |
| Correlated siblings | `build_join`, PullUpCorrelatedExpr and scalar plan invariants | New aggregate wrapping is restricted to uncorrelated input; existing correlated aggregate/decorrelation behavior remains intact |
| Missing aggregate registry | `scalar_subquery_to_join.rs:252-260,384-389,1288-1320` | Rule leaves input unrewritten rather than silently joining it as a set; Wyrd's normal registry includes the function |
| Runtime readiness | hash `stream.rs:624-703`; Wyrd `live.rs:298-318,435-478,676-690` | Completed build and global filter publication precede probe polling; lazy first-poll snapshot is correctly placed |
| Route ownership and wire | `live.rs::resolved`, `fragment`, `handle_child_pushdown_result` | Deal planned routes before runtime narrowing, serialize one immutable snapshot per partition, retain authoritative operators through unsupported pushdown claims |
| Session cost/ownership | `resources.rs:4090-4125`, `analytical.rs::planning_session`, `oracle/mod.rs::build_physical_root` | Two native optimizer passes repair equality extraction without a second planner/evaluator; no benchmark-backed latency claim is made here |
| Caller diagnostics | cumulative mapper and planning call sites in `oracle/mod.rs` | REQ-002 specifically requires caller planning faults as 4xx; runtime cardinality failure uses existing execution mapping, so 500 mapping is not expanded into a new domain finding |
| Integrated proof | peer scalar literal/footer, join, and two-row Filter/Projection journeys; compact Rust mixed-state corpus | Strong focused positive/negative proof for exercised shapes, but misses the FULL JOIN shortcut below |

`SINGLE_VALUE` is an established native relational approach: Apache Calcite's
[SqlSingleValueAggFunction documentation](https://calcite.apache.org/javadocAggregate/org/apache/calcite/sql/fun/SqlSingleValueAggFunction.html)
describes an aggregate that returns its sole input value and refuses multiple
inputs. The fork's explicit empty-input/NULL/duplicate semantics are assessed
against its native scalar extractor, rather than inferred solely from Calcite.
Default SQL visibility of this aggregate is reasonable native reuse, not a new
Wyrd public contract or duplicate evaluator.

The partial state has one typed value and a UInt64 row count. Empty partials
contribute zero; a singleton partial contributes one even for NULL. A partial
with multiple rows fails immediately, while multiple singleton partials fail
at final merge. Merge uses native array/scalar encoding; no new distributed
message, stage resource, scheduling owner, or worker-local cardinality guard
was introduced. Scalar aggregates already known to produce at most one row
remain unwrapped, preserving the R1 equality hash-filter path.

Dynamic filters remain conservative optimization inputs. Static conjunct
pruning happens during planning; each partition gets its stable route share
before runtime filtering. HashJoin waits for build collection and bounds
publication before polling its probe input, so first-poll snapshot sees the
completed equality filter. Native hash-lookup serialization can widen an
unserializable lookup to true; that affects pruning opportunities, not result
correctness. The earlier rejected demand to optimize scalar inequalities is
not reopened. The two-fragment/two-footer-row proof bounds outer transfer for
its equality query; it does not claim bounded physical scan work.

## Proposed finding

### DF-R3-1 — INCORRECT: the scalar cardinality check is skipped for an input that can emit two rows

**Mapped obligation:** REQ-006 scalar semantics; R2 AC-1/AC-2; prior
`FIND-bifrost-errors-3`. This is the same producer-owned cardinality failure,
not an unrelated general JOIN optimization request.

**Changed consumer:** DataFusion
`optimizer/src/scalar_subquery_to_join.rs:384-405` inserts `single_value` only
when `decorrelated_subquery.max_rows()` is unknown or greater than one.
**Invalid-state producer:** `expr/src/logical_plan/plan.rs:1487-1492` groups
FULL JOIN with LEFT/RIGHT JOIN and reports the product of known side bounds.
Two one-row unmatched FULL JOIN sides produce two null-extended rows, but the
method reports one. Projection/alias/filter/repartition/window wrappers can
propagate that value to the scalar rewrite. Existing
`logical_plan/invariants.rs:188` is a sibling consumer of the same bound.

**Reachable scenario:** analytical session disables physical uncorrelated
scalars. A caller evaluates:

```sql
SELECT (
  SELECT coalesce(l.x, r.x)
  FROM (VALUES (1)) l(x)
  FULL JOIN (VALUES (2)) r(x) ON l.x = r.x
);
```

The FULL JOIN's two unmatched rows must make the scalar fail. The bound of
one bypasses `single_value`; the scalar rewrite joins its input directly and
returns successful rows. `EliminateJoin` preserves FULL JOIN in this shape
(`eliminate_join.rs:395-420,513,604`); the traced final optimized plan confirms
the reachable path rather than relying only on a local arithmetic example.

**Focused reproduction:** orchestration authorized one existing-binary run,
with temporary input outside either candidate and no compilation. Fixture
`/tmp/bifrost-r3-cardinality/test_files/full_scalar.slt` sets the flag false,
first verifies `SELECT single_value(x) FROM (VALUES (1),(2)) t(x)` refuses
(it passes), then requires the FULL JOIN scalar above to refuse. Command:

```bash
cd /tmp/bifrost-r3-cardinality
env RUST_LOG=datafusion_optimizer=debug \
  /home/thorrester/Documents/GitHub/datafusion-scalar-snapshot/target/debug/deps/sqllogictests-5fda3bdc791d6a0c \
  --test-threads 1 full_scalar
```

Output retained in `/tmp/bifrost-r3-cardinality/output.log`: exactly one file
selected/executed, exit 1, with `query is expected to fail, but actually
succeed` for the scalar query. Trace shows a scalar LEFT JOIN over a FULL JOIN,
with no `single_value` aggregate. Fork worktree HEAD is f059acf6 and clean;
binary modification time is 18:26:12, before the 18:27:25 commit. Successful
first assertion verifies the new aggregate is registered. This is an existing
build with source/provenance evidence, not a fresh binary hash attestation.

**Observable consequence:** analytical scalar Filter/Projection can still
return successful wrong results if their input is incorrectly proved
singleton. The generic accumulator tests and published/live two-row journey
pass because those inputs have unknown/>1 bounds and therefore receive the
aggregate; they do not falsify this bypass.

**Smallest correction boundary:** repair the shared native
`LogicalPlan::max_rows` FULL JOIN upper-bound producer so it never advertises
at most one when unmatched rows can exceed that count; a conservative unknown
bound is acceptable when a correct bound is not established. Preserve valid
one-row aggregate/limit proofs and their optimized R1 path. Keep the existing
single-value rewrite and native final/coalesce integration. Do not add a Wyrd
guard, a FULL JOIN scalar allowlist, or independent consumer checks. Inspect
the complete bound producer and its existing correlated validation consumer,
so the correction is safe at the common source.

**Focused closure proof:** add a native bound regression for singleton
unmatched FULL JOIN, including propagation through a projection; run flag-off
Filter and Projection scalar cases over the FULL JOIN and assert refusal.
Retain singleton/empty input cases, equal/NULL duplicate checks, global
partial/final/codec checks, and R1 literal/footer proof. No benchmark or new
fixture framework is needed. A compact analytical-configured Bifrost query can
use constant VALUES inside a scalar alongside its existing authorized outer
table to prove the same rewrite configuration through the server.

## Verification limits and result

Recorded implementation evidence reports fork SQL/optimizer/aggregate/proto,
Wyrd focused units/journeys/corpus, and formatting/lint/codegen/boundary checks
passing. Their relevant source/assertions were inspected; no heavy lanes were
rerun by this reviewer. The single focused reproduction above was traced and
used normal one-test execution, with no benchmark or synthetic host load.

Prior FIND-1/FIND-2 remain closed within this domain for their validated scope.
FIND-3 is materially improved, but not closed because the new check trusts an
unsound cardinality producer. FIND-4 documentation is outside this domain's
independent standards responsibility. No new lifecycle/reservation diagnosis
is inferred from the earlier untraced green rerun.

**Overall result: FAIL**, with one proposed root-cause finding, DF-R3-1,
retaining prior `FIND-bifrost-errors-3` for independent validation.
