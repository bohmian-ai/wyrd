# Independent behavior review — round 3

## Subject and method

Root: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`.
Wyrd base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`, candidate
`dc353ef5ecf39da13f3dcac5b9b5a31558b6a35e`; DataFusion original
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`, candidate
`f059acf6c58b2c5fd4bc36b3358e8a56ac283476`; distributed fork
`8fac2a978bdcf8e06457238ebfe9c7f35023c123` unchanged.
Original task absent: approved spec revision 1, owner directions, R1 and R2
are the acceptance inputs. Reviewed cumulative implementation and fork changes,
using prior findings as hypotheses rather than accepting implementation claims.
CodeGraph was used before Wyrd code navigation. Candidate HEAD was checked and
remained the subject. Only this report is written.

Applicable authority: AGENTS, agent-rules, Wyrd design/doctrine, Bifrost design,
spec-driven development, maintainer style, DataFusion/OLAP-serving references,
error and testing workflow references. This is a behavior audit, not a fresh
architecture proposal. The original split spec's Jev/CLI/traceparent remnants
do not add non-Bifrost work. Private R1 timing/dealing instructions yield to the
stronger ownership/semantic contract and the round-2 validated disposition.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 logical caller faults carry planner detail/suggestion as 4xx | `oracle/mod.rs:3018-3033,3735-3797`: logical mapper traverses only context/diagnostic/shared/collection and accepted typed caller leaves; resource priority retained | mapper unit; Rust/Python/TS unknown-column single/join/subquery and mismatch/ambiguity journeys; MCP error journey | PASS |
| Internal and unauthorized schema details stay private | logical lowering runs after existing authorized cut/provider registration; physical lowering exposes fixed remediation; generic execution mapper remains opaque | retained scoped-reader/redaction journeys; mapper internal/resource assertions | PASS |
| REQ-006 scalar, IN, EXISTS including managed run_id correlations over live sources | existing native DataFusion execution and optimizer owners; live leaf snapshots recursive physical expression; fixture stamps shared RunId through real ingest | focused Rust/Python/TS/MCP equivalent-join journeys with explicit expected rows | PASS |
| INV-001 tenant isolation | unchanged binding/cut/ticket/fingerprint/peer resolution remains before fragment execution; test-only observation follows authenticated resolution | retained redaction journeys, recorded tenant-isolation check | PASS |
| R1 AC1 ready/pending/metadata/typed NULL/reset snapshots | fork `physical-expr/src/scalar_subquery.rs:169-184`; existing `Literal`, `FieldMetadata` and recursive snapshot contract reused | four snapshot tests and existing scalar tests; recorded exact fork command | PASS |
| R1 AC2 selective remote predicate ships as literal and bounds transfer in analytical configuration | two shared optimizer passes allow equality extraction; live Post filter uptake; first-poll snapshot after native hash build | `remote_scalar_subquery_ships_as_a_literal`: exactly two fragments/two footer rows and both time literals in encoded events fragment | PASS |
| R1 AC3 independent static pruning and disjoint routes | `live.rs:280-318,444-474,673-683`: static pruning at planning; assign planned routes by partition index before snapshot narrowing | `scalar_subquery_resolves_per_execution_after_static_pruning`: pending/ready/reset; dealt partition union equals narrowed eligible routes | PASS |
| R1 AC4 focused public surfaces retained | shared query client/server path; no SDK transport/runtime production changes | retained cross-surface tests and prior recorded suite evidence; later integrated capability gate still required | PASS within task proof |
| R1 AC5 no generated workload/oracle/public API or dependency promotion | cumulative diff has no support-workload framework; `wyrd-testing` DataFusion dependencies remain dev-only | source/manifests and recorded dependency check | PASS |
| Owner/R1 AC6 compact common analytical coverage incl CTE outer join | `query_coverage.rs`: small two-table fixtures, exact answers, mixed published/live, joins/aggregation/scalars/IN/EXISTS/CTE/window | exact corpus command and explicit query/expected rows | PASS |
| R1 AC7 representative remote live join | peer fixture reuses owned cluster, ingest and public client | `remote_live_join_returns_its_rows` asserts exact pair | PASS |
| R2 AC1 two-row analytical scalars refused in Filter and Projection | fork `ScalarSubqueryToJoin::build_join:382-404`, both caller branches; single_value producer before dependent join | flag-off SQL tests; remote SDK/server filter and projection refusal journey | PASS |
| R2 AC2 global/batch/duplicate cardinality, before evaluated scalar emits | `SingleValueAccumulator::update_batch/merge_batch/add`; ordinary final global aggregation distribution and coalesce | later-batch duplicate/NULL tests, merged partials, flag-off UNION ALL and no-match cases, proto roundtrip | PASS |
| R2 AC3 zero/one/typed NULL/type metadata semantics | typed NULL accumulator initialized from return type; one row retained even if NULL; return_field clones input type/metadata; at-most-one shortcut uses existing max_rows contract | zero/single/NULL unit cases; existing flag-off empty/NULL/singleton SQL cases, one-row-across-partitions/type query | PASS |
| R2 AC4 analytical journey spanning published/live and codec/collection compatibility | remote duplicate split published/live; default registered native UDAF, state fields value+count; native AggregateExec with Final SinglePartition | remote refusal, SQL explain Partial→Coalesce→Final, decoded plan refuses duplicate UNION ALL | PASS |
| R2 AC5 prior performance, public mapping, tenancy, protocol maintained | flag stays disabled; existing native aggregate/UDAF codec; fork pins and ORACLE_DATAFUSION_REVISION move together | retained literal/footer/join/corpus/mapper and codec tests; recorded checks | PASS |
| R2 AC6 missing Panics docs fixed; assertions preserved | mapper unit and MCP query-error journey now describe assertion panics; prior verdict blank line removed | source; cumulative diff-check performed in this review is clean | PASS |
| R2 AC1–3 every possibly multi-row scalar is validated | new max_rows shortcut trusts incorrect Full-join upper bound in expr logical_plan/plan.rs:1480-1494 | singleton unmatched FULL join emits two output rows despite Some(1); no added test covers bypass | FAIL, BEHAVIOR-1 |
| AC-003 focused checks/no benchmarks | committed exact-command evidence in R2, real named tests/targets present and assertions match claims | recorded fmt/lints/codegen/dependency/tenant checks, fork suites; review ran only diff-check | PASS for bounded task; final full gate deferred |
| Non-goals: no Wyrd evaluator, alternate planner, protocol, new resource owner, benchmark | native expression snapshot, native hash filter, native aggregate/state/registry/distribution reused; Wyrd remains fragment boundary owner | cumulative source and manifest inspection | PASS |

## Caller-to-result trace and attempts to falsify

### Scalar producer and both consumers

The analytical session builder still uses `with_distributed_planner`, which
disables physical uncorrelated scalar expressions. `ScalarSubqueryToJoin` is
therefore the producer to examine. Both Filter and Projection now retrieve the
same registered aggregate and invoke the same `build_join`. The helper checks
uncorrelated input after decorrelation; if `max_rows()` is unknown or greater
than one, it places a no-group `single_value` aggregate over the complete scalar
input before aliasing and joining it. This checks inner row multiplicity,
not matches in the outer join. If the registry lacks the aggregate, the rule
returns untransformed source instead of producing a wrong set join. Wyrd uses
default-feature SessionState owners that register this aggregate on leader and
followers. Correlated analyzer/decorrelation siblings are not rewritten by
this added condition and remain under their existing restrictions.

The intended `LogicalPlan::max_rows` contract is a guaranteed upper bound, not
scan statistics. LIMIT/Sort fetch and singleton global aggregates can safely
skip the wrapper. Unknown table sizes cannot: they take the aggregate path.
There is no DISTINCT or LIMIT truncation introduced by the wrapper.

However, the newly used contract has an incorrect FULL-join implementation:
two nonmatching singleton inputs report Some(1) although they return two rows.
The scalar wrapper therefore has a reachable cardinality bypass (BEHAVIOR-1).

The accumulator counts `values.len()`, so equal values and NULL are still rows.
Empty batches do not replace a value. A second row in the same batch or a later
batch raises the existing native scalar execution message before `evaluate`
can emit a row. Each successful partial state contains its zero-or-one row
count separately from its value, which is essential for distinguishing empty
input from one NULL row. `merge_batch` counts every state and refuses a second
row across partitions. Final/Single AggregateExec requires SinglePartition
(`physical-plan/src/aggregates/mod.rs:2066-2076`); the unchanged distributed
network boundary owner preserves coalescing above partials and Maximum(1)
(`distributed_planner/inject_network_boundaries.rs:364-397`). The ordinary
aggregate state codec/registry is reused; the decoded plan test executes and
refuses, rather than merely comparing display strings.

Native `execute_scalar_subquery` (`physical-plan/src/scalar_subquery.rs:397-436`)
provides the reference: typed NULL for empty, preserve one value including
NULL, refuse a second row across batches. The new aggregate matches those
semantics. Returning a successful typed NULL is not the same as suppressing
extra NULL rows. Tests cover that distinction. The correction does not promise
evaluation of work legitimately eliminated by the optimizer; the existing
no-matching-outer-value tests instead retain a reachable evaluated scalar.

### Remote selective execution

The shared two-pass config changes equality join selection, not SQL meaning.
A native hash-join scalar build completes before probe polling, visible in
`hash_join/stream.rs:689-698` (WaitBuildSide, partition-bound report, then
FetchProbeBatch). `LiveFragmentRead` resolves only inside its first-polled
stream, so fragment creation is late enough to carry the completed scalar or
hash filter. Planning preserves the runtime expression and can prune using
independent resolvable conjuncts. Partition ownership is fixed from the planned
route sequence before each snapshot only narrows its own share; there is no
route reindexing across different snapshots.

The remote test's events set has four hourly routes and 50 rows per route,
while the scalar source returns one instant. It checks two fragment opens and
two delivered footer rows plus literal contents, so a successful result with
a full-tail transfer cannot pass. Footer rows prove delivered/transport rows;
they do not establish physical source rows scanned, and this report makes no
scan-throughput or latency claim. The R2 aggregate bypass for MAX preserves
that existing selective hash-filter shape.

### Error and coverage boundary

The two-row scalar refusal occurs at execution, outside REQ-002's SQL planning
fault requirement. Keeping the existing opaque execution 500 is explicitly
preserved by R2 and does not require string-matching a dependency exception or
changing the public error catalog. Logical user mistakes retain the detailed
400 mapper, which avoids disclosing wrapper/internal context. Authorization
continues to precede registered provider schemas and SQL diagnostics.

The compact common-query corpus uses fixed expected rows rather than an
embedded second DataFusion oracle. It includes the owner-requested CTE whose
outer query joins a second real Bifrost table, nullable join/membership data,
and mixed published/live rows. Surface smoke journeys cover scalar/membership/
existence and managed run_id; the corpus is not duplicated across languages.
The new negative peer journey reuses one provisioned client for filter and
projection, fixing the documented duplicate key setup issue at its producer;
no weakened assertion, retry or timeout was found.

## Prior findings and proposed findings

- FIND-bifrost-errors-1: CLOSED for the diagnosed dropped scalar conjunct and
  analytical selective equality path. Native snapshot/filter propagation and
  footer proof are present. Previously rejected range optimization remains
  outside this finding.
- FIND-bifrost-errors-2: CLOSED. Oversized machinery is absent, compact owner-
  directed SQL coverage remains, and the testing dependency cone is unchanged.
- FIND-bifrost-errors-3: NOT FULLY CLOSED. Ordinary table inputs are correctly
  wrapped and validated, but the max_rows shortcut can classify an actual
  multi-row FULL-join scalar as singleton and retain wrong successful results.
- FIND-bifrost-errors-4: CLOSED. Both named test docs now contain substantive
  Panics sections without assertion changes.

### BEHAVIOR-1 — INCORRECT: invalid singleton bound bypasses scalar cardinality

Maps to prior FIND-bifrost-errors-3, R2 AC1–3, and REQ-006 scalar semantics.
Changed consumer: fork `optimizer/src/scalar_subquery_to_join.rs:383-385`.
Invalid-state producer: fork `expr/src/logical_plan/plan.rs:1480-1494`.

Reachable analytical/flag-off SQL:

```sql
SELECT (
  SELECT COALESCE(l.x, r.x)
  FROM (VALUES (1)) AS l(x)
  FULL JOIN (VALUES (2)) AS r(x) ON l.x = r.x
);
```

Each VALUES input reports `max_rows() == Some(1)`. The FULL join has no match
and emits `(1,NULL)` and `(NULL,2)`, so its projected scalar input has two rows,
values 1 and 2. `max_rows` groups FULL with Left/Right and returns
`left_max * right_max` when both are nonzero: Some(1). Projection and aliases
propagate that value. With no outer references the decorrelation walk retains
the join. The changed shared producer sees <=1 and skips single_value. It
joins that input to the outer singleton without checking scalar cardinality.
A scalar projection can therefore return two successful rows; Filter uses can
match either value. Native physical scalar execution rejects the second row.
Tests exercise unknown table sizes and unions but omit this bounded-input
bypass. This is a reachable missed correction, not optional SQL optimization.

Smallest correction: repair DataFusion's existing `LogicalPlan::max_rows`
FULL-join upper-bound producer so it never understates unmatched outer rows,
including zero and nonzero singleton sides. Trace other consumers of that
bound before selecting its implementation. Preserve the useful singleton
MAX/LIMIT bypass and existing scalar-to-join owner. No Wyrd guard or SQL
inspection is warranted. Wrapping every scalar indiscriminately would add
aggregation to known singleton MAX and disturb R1; fix the invalid producer.

Focused closure: unit-test the FULL upper bound with unmatched singleton
VALUES and zero/nonzero sides, and add flag-off scalar projection/filter
negative SQL cases with the unmatched singleton FULL input. Both must refuse
more than one row. Retain native scalar reference and existing literal/footer
proof. No benchmark or architecture decision is required.

No optional improvement is proposed as a blocking defect. Registering
SQL-callable single_value is ordinary
native aggregate integration needed by default registry and codec consumers;
its exposure alone is not evidence of a separate Wyrd product contract.

## Verification limits and result

No benchmarks, synthetic host load, Cargo compilation, live-environment tests,
or heavy suites were run during this discovery review. Test registration,
source assertions and committed exact-command evidence were inspected. Recorded
R2 green evidence is credible for the bounded write set; later change review
owns complete integrated verification. Earlier unexplained 604-byte reservation
observation has no proven changed source cause here and is not diagnosed by a
later pass. Unsupported flag-off scalar positions (ORDER BY/JOIN ON/aggregate
arguments) remain documented engine limitations; this correction does not
introduce those paths or claim exhaustive SQL support.

Overall result: **FAIL** (BEHAVIOR-1; prior FIND-bifrost-errors-3 remains open).
