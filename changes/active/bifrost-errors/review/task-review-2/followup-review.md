# Focused follow-up: scalar support scope and analytical execution

## Subject and uncertainty

Base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`; candidate
`7d6a1ff95b3dc09315e3af236ab9906b305e11d0`; HEAD remains that candidate.
Read the behavior, invariant and DataFusion discovery reports, approved spec,
prior validated ledger, R1 task/evidence, cumulative diff and governing
spec-driven-development/DataFusion/OLAP references. Started Wyrd navigation
with CodeGraph, then inspected the pinned DataFusion and distributed producer
paths. No benchmarks, executable test reruns or candidate edits were made.

The conflict is whether the equality closure proves the diagnosed predicate
defect, and whether general scalar support can be accepted with a known
successful-wrong-result edge case. These are different obligations and should
not be collapsed into a demand that every scalar query receive the same
optimization.

## Resolved scope

### DF-R2-1: range performance limitation is real, but not a new blocking finding

The source trace in the domain report is correct. Distributed
`src/distributed_planner/session_state_builder_ext.rs:25-29` disables physical
uncorrelated scalars; DataFusion
`optimizer/src/scalar_subquery_to_join.rs:389-391` constructs the join;
equijoin extraction does not turn `>=` into an equality key. A no-key plan can
therefore remain a nested-loop join. Its
`physical-plan/src/joins/nested_loop_join.rs:841-913` pushdown hook only
forwards parent dynamic predicates, while `execute` collects the left input
and constructs the right probe stream. No scalar-range producer is present.
The current equality footer test does not prove range pushdown.

However, this is insufficient to establish a separate required correction.
The original diagnosis was removal of available scalar-dependent predicates
from the live leaf, not absence of every possible join runtime optimization.
R1 criterion 2 asks for one highly selective scalar scenario, and its cited
example is equality. The owner requires principled, performance-safe support;
that does not require creating runtime range filters for every pre-existing
join shape. The distributed flag and no-key join mechanism are unchanged.
The cumulative candidate removes the concrete exclusion, retains available
predicates and improves an existing equality execution path. No new range
slowdown relative to that existing configuration is established.

The direct `>=` scalar unit is still valid for recursive physical scalar
snapshotting; it must not be represented as an analytical range execution
test. Findings 1's equality closure can stand, with the explicit limit that
analytical scalar inequalities can transfer the full eligible live tail.
Do not require a new NLJ producer solely to make performance exhaustive, and
do not describe that limitation as fixed. Proposed DF-R2-1 is rejected as a
blocking finding under this bounded task, rather than rejecting its factual
execution trace.

### DF-R2-2: scalar cardinality is intrinsic correctness, not optional optimization

The approved REQ-006 names scalar subqueries without a restriction to
aggregate-produced singleton inputs. The owner also asks for common query
coverage and behavior that works as expected. Unlike efficient range
execution, refusing multiple rows is part of what makes this a scalar
expression. The phrase "same rows as the equivalent join" cannot authorize
replacing scalar expressions with unrestricted set joins: those are not
equivalent when the supposed scalar has multiple rows.

Pinned DataFusion `common/src/config.rs:1626-1640` expressly states that the
false flag silently produces incorrect multi-row results. Its ordinary
`physical-plan/src/scalar_subquery.rs:397-436` extraction owner reads the whole
scalar stream, returns a typed NULL on zero rows, accepts exactly one row and
rejects another row even in a later batch. That behavior is bypassed by the
legacy scalar-to-join producer. Two optimizer passes merely change the join
algorithm; they cannot restore the missing cardinality invariant.

Reachable case using the new fixture:

```sql
SELECT order_id
FROM vala.datasets.coverage_orders
WHERE amount = (
  SELECT amount FROM vala.datasets.coverage_orders WHERE order_id IN (10, 11)
)
ORDER BY order_id
```

The inner relation contains 50 and 30. With the analytical configuration the
producer creates a relation join, so matching outer rows are successful
results rather than a scalar-cardinality error. The retained join is the
consumer applying set semantics, so it cannot repair this. The corpus at
`query_coverage.rs:188-212` checks singleton, empty and NULL cases but omits
the contradictory multi-row case.

The flag predates the task, but the missing behavior is within the exact scalar
capability being accepted, not unrelated debt. DF-R2-2 remains a proposed
INCORRECT finding for independent validation. It should receive the next
unused FIND ID, rather than reopening the prior predicate-loss ID.

## Smallest correction boundary for the retained proposal

Keep the distributed safety flag disabled. Correct the DataFusion
scalar-to-join rewrite so its uncorrelated scalar input retains runtime
zero/one/multiple-row semantics before dependent rows can be emitted. Reuse
the semantics of the existing scalar extraction owner: globally validate the
complete input, emit one typed scalar row (including typed NULL on empty
input), or fail on a second row. The invariant belongs at that producer, not
in Wyrd's live leaf, SDKs, or result normalization. An at-most-one assertion
after ordinary join output is too late and can hide errors when no outer rows
match.

The required enforcement must cover the entire scalar input across partitions
and batches, not independently validate worker-local subsets. Keep it a native
DataFusion relational dependency compatible with existing coalescing and
codec ownership, so both hash and no-key consumers receive the same validated
input. Do not add a new stage protocol, turn the physical-scalar flag back on,
or use LIMIT 1/first-value aggregation. Those either reintroduce the documented
state-binding hazard or change SQL results.

This is a bounded engine semantic correction under REQ-006. It does not
authorize changing Wyrd admission, query-class selection, stage execution,
resource sharing, failure policy or tenant boundaries. R1's private prescribed
mechanics cannot waive scalar correctness; any needed task correction should
be routed through the canonical planning workflow. If independent validation
establishes that enforcing this native relational dependency actually requires
a new distributed scheduling/resource protocol, that material choice must
return SPEC_REVISION_REQUIRED rather than being invented in remediation.
This report finds no basis to prescribe such a protocol.

Focused closure proof: a real analytical-configured server query rejects two
scalar rows, including rows separated across published/live inputs or batches;
the existing singleton, empty and NULL cases remain correct. A fork-focused
execution regression must show global refusal before dependent output. Keep
the equality fragment/counter proof; no benchmark or broad lane is necessary
to prove cardinality.

## Optimizer budget

Changing one pass to two is bounded structural optimizer work at the existing
session owner (`resources.rs:4090-4123`). The rule-order explanation and the
hash-join execution trace support the correction. The historical comment's
microsecond estimate is not a measurement of this candidate. Neither the
owner's instruction nor R1 reasonably means zero additional planning latency:
repairing a plan may cost an extra traversal while eliminating unbounded
execution work. No benchmark is needed or authorized here. Do not claim a
measured latency guarantee from that comment.

## Result

**RESOLVED.** Equality predicate closure and compact coverage can be accepted
on their actual evidence; range optimization remains an accurately stated
non-blocking limit. General scalar correctness still has a source-backed
multi-row gap, proposed as DF-R2-2 for independent validation. No new stage
protocol or speculative lifecycle correction is recommended.
