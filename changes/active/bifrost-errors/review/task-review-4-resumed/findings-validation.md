# Independent structured Ponytail validation — resumed round 4

## Subject and method

Wyrd candidate `17697dc8c2740213565e5f913cde1edfb0cb8ce5`, integration
baseline `945a1548c47897dc94583169a6ba04163a73a749`; original task base
`526355fb8c58dae6d67d516f98dddcd5c02b0e79`, pre-merge task candidate
`7fe69545d0d8a2ab2a1e3033f52d5091e80d5b0e`. Pinned DataFusion original
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`, candidate
`b2d37bc182b9d3e576650a414b364f3d9b5aefe3`; distributed fork unchanged
at `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

The owner explicitly authorized continuing after the main merge. This report
validates the resumed subject rather than asserting that the interrupted
subject never moved. The prior interrupted reports remain preserved.

Inputs include the resumed subject/claim comparison, all seven resumed
discovery reports, approved spec, absence of original task, owner steering,
R1–R3 tasks/evidence and prior validation/verdicts. Governing authority is
AGENTS, agent-rules, spec-driven development, maintainer guidance and applicable
native DataFusion/OLAP, tenant, public-error and test boundaries. CodeGraph
preceded Wyrd source navigation. Cumulative diffs and native source, not the
implementation summary or reviewer agreement, establish the conclusions below.
Only this report is written; no candidate source or test fixture was edited.

## Proposal dispositions

| Proposal | Disposition | Independent source assessment |
|---|---|---|
| Behavior BEH-1 | CONFIRMED | `Filter::is_scalar` accepts key equality to an arbitrary non-Column expression; an expression varying with each input row does not fix a unique key. |
| Invariant INV-R4-1 / comparison INV-1 | CONFIRMED | The same producer supplies a false `Some(1)` through projection/alias to both scalar rewrite callers. |
| DataFusion DF-R4-1 | CONFIRMED; correction REVISED | Recorded traced reproduction corroborates optimizer survival and successful wrong results. Repair the existing proof producer conservatively; separate column/volatility checks are insufficient as a complete fixed-value proof. |
| Standards, maintainer, system and tenancy | No material proposal to retain | Their source coverage and scope limits are present; no separate finding is inferred from their PASS results. |

These are manifestations of the same still-open **FIND-bifrost-errors-3**.
No new ID is assigned per expression, query position or reviewer. The human's
request for safe construction of proof state remains authority, not a
retroactively invented independent finding.

## Validated finding ledger

### FIND-bifrost-errors-3 — CONFIRMED, correction REVISED

- Discovery IDs: BEH-1, INV-R4-1 (comparison INV-1), DF-R4-1.
- Classification: INCORRECT; incomplete closure of the existing finding.
- Obligation: REQ-006 and R2 evaluated-scalar cardinality, R3's explicit
  sound-proof invariant: runtime validation may be skipped only when the
  complete input is guaranteed to have at most one row.
- Invalid-state producer: pinned fork
  `datafusion/expr/src/logical_plan/plan.rs:2909–2969`,
  `Filter::is_scalar`, especially its equality match at :2943–2952.
- Bound propagation: `LogicalPlan::max_rows` at :1453–1458;
  Projection/SubqueryAlias propagate the returned bound.
- Changed consumer: `datafusion/optimizer/src/scalar_subquery_to_join.rs:
  383–405`, shared by Filter and Projection rewrite branches.
- Sibling consumer: `datafusion/expr/src/logical_plan/invariants.rs:188`
  also trusts this bound for a nonaggregated correlated scalar input.
- Consequence: an evaluated multi-row scalar can still return successful
  wrong results under Analytical session configuration because the correct
  native runtime checker is omitted.

### Producer-to-consumer proof

Read the complete `max_rows` match, `Filter::is_scalar`, shared `build_join`
and both rewrite call sites, scalar invariant consumer, Aggregate dependency
construction and unique-key dependency producer. The functional dependency is
legitimate: ordinary GROUP BY establishes a nonnullable Single dependency
from its group key to the full output (`common/src/functional_dependencies.rs:
487–508`). The filter then misuses that fact.

For every equality conjunct, the current helper accepts a Column on one side
and anything except a bare Column on the other. It never proves that the
other expression has one fixed value for the whole input. A unique key
constrained to a different expression result for each row may retain many
rows. Composite-key coverage and nullable-key checks do not repair that error.

This concrete input retains two groups: `(x,n)=(1,1)` and `(2,2)`.

```sql
SELECT (SELECT x
        FROM (SELECT x, COUNT(*) n
              FROM (VALUES (1), (2), (2)) t(x)
              GROUP BY x) g
        WHERE x = abs(n)) AS picked
FROM (VALUES (0)) o(z);
```

The predicate cannot simply move beneath its Aggregate because it uses the
aggregate count. The unique key x changes for each retained row, while the
opposing expression changes with it. `is_scalar` nevertheless returns true,
`max_rows` manufactures Some(1), and shared `build_join` skips `single_value`.
The resulting LEFT JOIN has set semantics and cannot enforce the erased
scalar contract. Arithmetic and cast wrappers around input columns share the
same defect; this is not an `abs`-specific bug.

The native query becomes a public Wyrd query by replacing the outer VALUES
with the existing nonempty authorized events table. Current
`parse_select_tables` discovers and validates real table references, requires
one Bifrost relation and has no inner-expression allowlist that excludes this
query. Existing authorization precedes session registration; VALUES needs no
separate table privilege. Analytical `planning_session` retains the pinned
distributed configuration disabling physical uncorrelated scalars. Thus the
reported path is reachable and required, not a dormant test-only surface.

### Execution corroboration and limits

Independently read `/tmp/bifrost-r4-filter-proof/test_files/filter_scalar.slt`
and `/tmp/bifrost-r4-filter-proof/output.log`. The one-file fixture disables
physical uncorrelated scalars, verifies direct `single_value` refuses two rows,
then expects the query above to refuse. The log completes exactly 1/1 files,
exits with `query is expected to fail, but actually succeed`, and shows the
final optimized LEFT JOIN over the grouped filter without `single_value`.
Two optimizer passes retain this shape. This directly corroborates the source
trace rather than merely exhibiting a false bound on a plan later optimized
away.

This is discovery's previously executed, traced existing-binary reproduction;
validation did not rerun it. The unchanged fork/source and passing registered
aggregate control support applicability, but the binary was not rebuilt or
hash-attested by this validator. Exact command and binary provenance are in
`domain-review-datafusion.md`. The defect is independently established by the
current source contract as well as that execution evidence.

### Decision-complete smallest correction

Repair **`Filter::is_scalar` in the DataFusion fork**, the existing producer
that owns the proof. Select a conservative positive proof of a value fixed
for the complete filter invocation, using the existing native expression
tree and contracts, in either operand order. Fixed leaves include literals,
bound parameters and typed OuterReferenceColumn values. Ordinary pure native
operators/casts preserve that property only when every operand is fixed;
immutable/stable scalar calls require fixed arguments and their native
volatility contract. Input Column dependence, per-row volatility, scalar
variables or opaque nested-plan/aggregate/window/higher-order forms cannot
contribute proof merely because they are not bare Columns. Return no new
proof for an expression whose invocation invariance is not established.
Retain existing conjunction, composite-key completeness and nullable-
dependency semantics. Reuse ordinary constant folding; do not evaluate
expressions or invoke the optimizer recursively inside the bound owner.
The existing `max_rows` fallback retains the input bound, which may
independently be soundly singleton.

This deliberately chooses one small safe approach instead of prescribing a
new expression-constancy framework. The installed `any_column_refs`,
`contains_outer`, `contains_scalar_subquery` and `is_volatile` expose separate
facts, not a complete invocation-invariance contract. In particular,
`is_volatile_node` checks ScalarFunction only, while native expressions also
include HigherOrderFunction and runtime-bound/nested-plan forms. No-columns
plus nonvolatile is therefore not a sufficient replacement proof. Native
ConstEvaluator already enumerates node eligibility and folds eligible
immutable/stable constants; duplicating its classifier here is unnecessary.

This is a bounded repair to the existing proof classifier, not a second
classifier at its consumer or a new framework. Literal-only would be
insufficient as a no-regression correction: `subquery.slt:439` already pins
nonaggregated correlated lookup by primary key (`t3.t3_id = outer_ref(t1.id)`),
and `check_subquery_expr` admits that legal plan through this very bound.
An outer reference varies across outer invocations but is fixed within one
inner invocation. Preserve that behavior, including ordinary fixed arithmetic,
cast and immutable-function compositions of outer values, while rejecting
dependence on the filtered input. Keep valid literal equality, genuine global
MAX and LIMIT/fetch singleton proofs and normal parameter substitution. No
parameter store or evaluator is needed. Preserve existing aggregate/run_id
correlation too. This prevents the confirmed unsafe proof construction but
does not claim no future programmer can introduce an unsound proof elsewhere.

Unknown/multi-row input already causes the shared native rewrite to insert
the existing `single_value`. Keep that accumulator, count state, normal
partial/final merge, default registry and native codec. No Wyrd predicate
guard, SQL-shape exception, new node/protocol, result store, dependency or proof
type is required. Preserve the two-pass session, disabled distributed scalar
flag, hash-build readiness, first-poll snapshot, static pruning and disjoint
planned route ownership. Advance the existing Wyrd fork pin, lock and codec
revision fence together.

### Focused closure proof

1. Extend native bound/Filter tests using a nonnullable grouped unique key
   with two surviving rows. Column-dependent arithmetic, function and cast
   operands must not manufacture Some(1); literal equality in both orders
   and complete composite-key equality remain positive controls. Retain the
   existing nonaggregated correlated primary-key lookup and fixed outer-
   reference arithmetic/cast/immutable-function controls; varying inner
   columns must never be confused with invocation-fixed outer values.
2. A volatile or otherwise unresolved runtime/nested expression must not
   supply this proof. Use ordinary native test expressions rather than
   probabilistic random output as the assertion. Preserve nullable-key and
   missing-composite-key behavior and genuine MAX/LIMIT controls.
3. Native flag-off SQL Filter and Projection positions for the concrete
   count/abs input must refuse a second row. Include the physical-scalar
   reference and normal folded-constant controls, keeping existing FULL,
   grouping, overflow, empty, equal/NULL and partial/final/codec cases green.
4. Extend the existing Analytical-configured public negative peer journey
   with a real authorized outer table, reusing its existing client/fixture.
   Retain exact literal/footer, remote join, compact corpus and mapper proof.
   Only normal-concurrency focused changed-owner lanes are needed; no
   benchmark, new harness or broad journey aggregate in remediation.

## Prior closure and rejected scope expansion

- FIND-1 remains CLOSED for its diagnosed dropped available selective
  predicate. Current live owner accepts Pre static and Post dynamic filters,
  retains the original predicate, snapshots once per partition on first poll,
  and partitions planned routes before narrowing. Native hash join waits for
  build/filter reports before polling the probe. Actual peer assertions count
  two fragments/two footer rows and inspect literal bounds. This proves
  bounded transfer for that selective shape, not all scan IO or wall time.
- FIND-2 remains CLOSED. Fixed small published/live fixtures and expected
  answers cover joins, joined CTE, memberships, correlated expressions,
  grouping, windows and scalar controls; the generated oracle/framework and
  ordinary `wyrd-testing` DataFusion dependency are absent.
- FIND-3 remains OPEN only for the shared unsafe proof above. FULL JOIN,
  GROUPING SETS and checked arithmetic corrections are sound and retained;
  the accumulator/global count mechanism does not need replacement.
- FIND-4 remains CLOSED. Mapper and MCP tests contain the previously missing
  Panics documentation, retaining their assertions.
- Opaque runtime 500 mapping is explicitly preserved; REQ-002 concerns caller
  planning failures. No new error-mapping finding is retained.
- The earlier untraced reservation observation remains unexplained evidence,
  not a newly established leak. No weakened assertion/retry/timeout correction
  is found. No speculative availability or cleanup finding is added.

The integration comparison and resumed reports inspect gateway/AppState,
test-server, MCP discovery/security, lock and mise seams. Direct current source
retains Bifrost planning, fragment, authorization and journey owners; the
unchanged native fork retains the reproduced defect. Unrelated main changes
are not recast as task additions. Earlier green execution evidence precedes
the merge; this report does not claim a new merged-server execution run.

## Result

One bounded implementation finding remains: **FIND-bifrost-errors-3**.
No new material product, public API, resource, concurrency or persistence
decision is needed for the correction. Final ledger is nonempty; validated
result **FIX_REQUIRED**. No further finding ID is assigned.

Skipped: no benchmarks, builds, broad lanes or repeated reproduction.
Risk: evaluated analytical scalars can still return successful wrong results
through the remaining false singleton proof until the native producer is fixed.
