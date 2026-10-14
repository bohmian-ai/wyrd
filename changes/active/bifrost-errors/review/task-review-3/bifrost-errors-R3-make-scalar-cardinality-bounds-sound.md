---
id: bifrost-errors-R3
kind: remediation
status: ready
spec: changes/active/bifrost-errors/spec.md
spec_revision: 1
requirements: [REQ-006, INV-001, AC-003]
parent_task: bifrost-errors-R2
remediates: [FIND-bifrost-errors-3]
---

# Make DataFusion's singleton proof sound at its owner

## Authority and immutable subject

- Approved spec: `changes/active/bifrost-errors/spec.md`, revision 1.
- Original task: absent; approved spec and owner instructions govern.
- Original Wyrd base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Reviewed Wyrd candidate: `dc353ef5ecf39da13f3dcac5b9b5a31558b6a35e`.
- Reviewed DataFusion: `f059acf6c58b2c5fd4bc36b3358e8a56ac283476`.
- Distributed fork: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`, unchanged.
- Prior task: `review/task-review-2/bifrost-errors-R2-enforce-analytical-scalar-cardinality.md`.
- Current verdict/ledger: `review/task-review-3/{verdict,findings-validation}.md`.
- Earlier task and verdict/ledger: `review/task-review-1` and `review/task-review-2`.

Route directly to `$wyrd-implement`. The owner additionally requires preventing
unsafe edge states at their owning boundary, rather than handling query shapes
with downstream guards. This task corrects that same existing FIND-3 invariant.

## Diagnosis: the scalar validator is skipped using a false singleton proof

The new native single_value aggregate correctly counts rows, including
duplicates and NULLs, across batches and final merges. However, the shared
scalar-to-join producer only inserts it when max_rows is unknown or greater
than one (`datafusion/optimizer/src/scalar_subquery_to_join.rs:383-405`).

The proof owner, LogicalPlan::max_rows
(`datafusion/expr/src/logical_plan/plan.rs:1446-1535`), promises a guaranteed
upper bound. Two row-expanding branches violate that contract:

- FULL JOIN shares the left/right product bound. Two unmatched singleton
  inputs produce two null-extended rows while reporting Some(1).
- Grouping-set Aggregate forwards its input bound. GROUPING SETS ((x), ())
  over one input row produces two groups while reporting Some(1).

Projection and alias nodes propagate these false bounds. Filter and Projection
both consume the bound through build_join, and the correlated scalar invariant
validator at `logical_plan/invariants.rs:172-199` is a sibling consumer. Once
the rewrite trusts the false singleton proof, later optimizer passes no longer
have a scalar expression to validate. The retained join applies unrestricted
set semantics and cannot repair this failure.

For example, with the existing registered peer events table as `{events}`:

```sql
SELECT id,
       (SELECT COALESCE(l.x, r.x)
        FROM (VALUES (1)) l(x)
        FULL JOIN (VALUES (2)) r(x) ON l.x = r.x) AS picked
FROM vala.bifrost.{events};

SELECT id,
       (SELECT COUNT(*) FROM (VALUES (1)) t(x)
        GROUP BY GROUPING SETS ((x), ())) AS picked
FROM vala.bifrost.{events};
```

Both evaluated scalar inputs must refuse a second row. The first returns two
different values; the second returns two identical count values. Analytical
configuration instead allows successful wrong results because single_value
is absent. An authorized real outer table makes these paths reachable through
Wyrd's SELECT floor; this is not restricted to table-free engine fixtures.

Discovery and independent validation each ran one traced existing-binary SQL
file outside the candidates. Direct single_value controls refused two rows;
the FULL/grouping scalar expectations each failed because execution succeeded.
Their traces retain the expanding relation under the scalar join with no
single_value. Existing-binary provenance limits and exact commands are recorded
in the current domain/validation reports. The source contract independently
establishes the defect.

## Intended outcome and minimum correction

Repair DataFusion's one native upper-bound proof owner. Some(n) must mean the
plan cannot emit more than n rows; an uncertain cardinality remains None.
The scalar invariant is: skip single_value only when this owner soundly proves
that the complete input cannot exceed one row; otherwise retain runtime
cardinality validation. An estimate is not proof.
Select the conservative correction: FULL JOIN and grouping-set expansion
return unknown unless a sound bound is established. No new bound computation
is necessary for closure. Unknown already routes both scalar consumers through
the existing single_value validation, making the unsafe singleton proof
unavailable without a Wyrd guard or second classifier.

Within this same owner, make derived numeric bounds conservative under empty
inputs and overflow. Use checked join multiplication and union accumulation;
return unknown when a bound is not representable instead of wrapping or
panicking. Audit the complete closed operator match and its propagation so
the same false proof is not produced by a sibling row-expanding branch.
Preserve sound bounds for nonexpanding operators, ordinary aggregates, VALUES
and LIMIT/fetch, especially genuinely singleton global MAX and LIMIT 1 inputs.
Their existing bypass and R1 equality runtime-filter proof remain useful.

This correction prevents the confirmed FULL JOIN and grouping-set inputs from
being falsely classified as singletons. It does not make every future
incorrect bound impossible to express at the type level. The proof owner and
its operator-specific derivations must remain sound as code evolves; regression
tests protect the identified cases. Do not introduce a new proof type merely
to claim that guarantee: a type cannot repair an unsound constructor.

Keep the existing single_value accumulator, row-count state, default registry,
native AggregateUDF codec and global Partial/coalesce/Final execution. The
bug is in the proof used to omit validation, not those owners. Do not replace
them with a new relational node, framework, shared result store or protocol.
Advance Wyrd's fork pins, lock and codec revision fence together using the
existing dependency update path.

This is a bounded semantic correction at an existing owner. It does not
require a public API, tenancy, concurrency, resource or stage-protocol decision.
Ordinary FULL JOIN and grouping queries remain allowed to return multiple rows;
only their invalid use as a scalar is refused.

## Preserved behavior and non-goals

- No per-query-shape exceptions in the scalar rewrite or Wyrd, SQL inspection,
  allowlist, DISTINCT/LIMIT masking, or second singleton-proof implementation.
- Preserve singleton/empty/typed-NULL semantics, duplicate-row cardinality,
  Filter/Projection producer sharing and existing correlated analyzer behavior.
- Preserve genuine singleton optimizations; do not indiscriminately wrap every
  scalar in an aggregate.
- Preserve the distributed physical-scalar flag, shared two-pass session,
  authoritative join/filter and native global count/codec contracts.
- Preserve first-poll snapshot readiness, static pruning, immutable predicate
  fragments and disjoint planned route ownership.
- Preserve tenant bindings, admission, memory/spill ownership, cancellation,
  deadlines and existing opaque runtime error mapping.
- Do not require execution of optimizer-eliminated scalar work; qualify an
  evaluated dependency against the native scalar reference.
- Do not reopen scalar inequality optimization or introduce a new test framework.
- No benchmark, synthetic load, raised concurrency or wall-clock threshold.

## Acceptance criteria

1. FIND-3: max_rows cannot prove at most one for the disjoint singleton FULL
   JOIN or singleton grouping-set examples; bound propagation through
   projection/alias remains conservative. Unknown is sufficient. The scalar
   rewrite bypasses validation only on a sound at-most-one proof, never a row
   estimate or uncertain bound.
2. FIND-3: the same owner handles empty inputs and numeric overflow without
   understating output or panicking; derived bounds remain proof, not estimates.
3. FIND-3: analytical/flag-off scalar Filter and Projection over both expanding
   inputs refuse a second row, including equal grouping-set values. The existing
   validator is inserted automatically from the corrected native proof.
4. FIND-3: an existing analytical-configured public peer journey exercises a
   real authorized outer table with a falsely bounded scalar shape and refuses
   it; extend existing fixed fixtures/client setup rather than a framework.
5. Genuine singleton MAX/LIMIT proofs and existing empty, NULL, duplicate,
   cross-batch/global-state and native codec tests stay correct. R1's literal/
   footer proof, representative remote join, compact corpus and mapper remain
   green. No new protocol, resource owner or flag reversal appears.

## Focused proof and verification

Use Red-Green-Refactor: first pin the unsafe max_rows results and flag-off
successful wrong scalar outcomes, then repair the native proof owner and
demonstrate refusal. Add direct bound cases for both expanding operators,
projection/alias propagation, empty inputs and arithmetic overflow, with true
singleton controls. Add native SQL Filter/Projection refusal cases for both
operators; retain existing scalar and codec regressions. Run normal concurrency
through the fork's pinned toolchain. Record exact package/target/selectors for
each new named test and prove the selectors execute tests.

Add the public negative case to the existing analytical peer journey and record
and run its exact `mise exec -- cargo nextest run --locked` selector with the
repository-managed Postgres wrapper. Retain these exact regression commands:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::live::tests::scalar_subquery_resolves_per_execution_after_static_pruning) | test(=oracle::tests::datafusion_query_rejections_are_safe_and_actionable)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::live_subquery::remote_two_row_scalar_subquery_is_refused) | test(=peer_network::live_subquery::remote_scalar_subquery_ships_as_a_literal) | test(=peer_network::live_subquery::remote_live_join_returns_its_rows)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=query_coverage::common_queries_answer_over_published_and_live_rows)'"
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(/^oracle::codec::/)'
```

Run affected fork bound/rewrite/SQL tests and its canonical format/lint checks
for the actual write set, with exact selectors recorded in completion evidence.
Run Wyrd `mise run fmt`, `mise run lints`, `mise run codegen:check`,
`mise run check:deps`, and `mise run check:tenant-isolation` for the dependency
and test changes. Check the cumulative diff from the original Wyrd base.
Keep Cargo-backed commands sequential across shared targets. No whole journey
suite or broad correctness aggregate is required in this remediation; later
integrated change review runs the full Bifrost capability gate once.

## Implementation evidence

Status: IMPLEMENTED. DataFusion fork `wyrd-production` at
`b2d37bc182b9d3e576650a414b364f3d9b5aefe3` (`75df30138` fix, `b2d37bc18`
clippy-only test tidy); distributed fork unchanged at `8fac2a978…`. Wyrd
`6722aeec4` advances pins, lock and `ORACLE_DATAFUSION_REVISION` together and
extends the existing journey.

Owner and gap: `LogicalPlan::max_rows` (`datafusion/expr/src/logical_plan/plan.rs`)
is the only bound producer; its consumers are `ScalarSubqueryToJoin::build_join`
and `logical_plan/invariants.rs`, both of which already route `None` to runtime
validation. Audit of the closed match: FULL JOIN with both sides non-empty fell
to the product; grouping-set Aggregate forwarded its input bound; Inner/outer
join multiplication and Union accumulation were unchecked. All other branches
are non-expanding (Projection, Window, Sort, Repartition, Distinct, SubqueryAlias),
exact (Values, EmptyRelation, Limit/TableScan fetch), or preserve the left side
once (semi/anti/mark, AsOfJoin). `Filter::is_scalar` relies on unique-key
functional dependencies, which FULL joins and grouping sets already drop and
other joins downgrade to `Multi`, so it is sound. Correction: grouping-set
Aggregate and FULL JOIN with two non-empty sides return `None`; empty-side
FULL/LEFT/RIGHT keep the other side's exact bound; join products use
`checked_mul` and Union uses `checked_add`. The rustdoc now states that
`Some(n)` is a guarantee. No change to `single_value`, the rewrite, the codec
or execution.

Red: on the old owner, `max_rows_is_a_sound_upper_bound` failed (FULL JOIN
reported `Some(1)`); the four new flag-off `subquery.slt` cases succeeded;
and `remote_two_row_scalar_subquery_is_refused` returned 400 rows for the FULL
JOIN projection. Green: the traced journey (`WYRD_LOG=info,vala_bifrost_redux=debug`)
logs `Scalar subquery returned more than one row` for all four queries, and the
client receives `WYRD_VALA_500_QUERY_EXECUTION_FAILED`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1 No false at-most-one for FULL JOIN or grouping sets; Projection/alias conservative; bypass only on sound proof | `max_rows` FULL and grouping-set branches return `None` | `logical_plan::plan::tests::max_rows_is_a_sound_upper_bound` (both operators, Projection+alias over each) | PASS |
| 2 Empty inputs and overflow neither understate nor panic | empty-side join arms kept; `checked_mul`/`checked_add` | same test: FULL/Inner with a `LIMIT 0` side give `Some(1)`/`Some(0)`; cross join and Union of `usize::MAX` scans give `None` | PASS |
| 3 Flag-off Filter and Projection refuse both expanding inputs, including equal grouping-set values | validator inserted automatically from the corrected bound | fork `subquery.slt` four new cases (Red before, Green after) | PASS |
| 4 Analytical public peer journey with a real authorized outer table refuses a falsely bounded shape | FULL JOIN and GROUPING SETS projections over the registered events table added to the existing journey | `peer_network::live_subquery::remote_two_row_scalar_subquery_is_refused` | PASS |
| 5 MAX/LIMIT singleton proofs, empty/NULL/duplicate/global/codec, R1 literal/footer, remote join, corpus, mapper unchanged; no protocol/flag change | global aggregate and LIMIT bounds untouched | bound test controls; slt `max` EXPLAIN shows no `single_value`; existing slt LIMIT/empty/NULL/duplicate cases; `single_value` and proto tests; journeys, corpus, mapper and codec commands below | PASS |

Commands (all exit 0):

- Fork (`~/Documents/GitHub/datafusion-scalar-snapshot`, pinned toolchain 1.98.1; `cargo-nextest` is not installed there, so exact `cargo test … -- --exact`):
  - `cargo test -p datafusion-expr --lib logical_plan::plan::tests::max_rows_is_a_sound_upper_bound -- --exact` (1 passed)
  - `cargo test -p datafusion-sqllogictest --test sqllogictests` (520/520 files)
  - `cargo test -p datafusion-expr --lib` (264), `-p datafusion-optimizer --lib` (886), `-p datafusion-functions-aggregate --lib` (245), `-p datafusion-proto --test proto_integration` (265)
  - `cargo fmt --all -- --check`; `cargo clippy -p datafusion-expr --all-targets --all-features -- -D warnings`
- Wyrd:
  - `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(=oracle::live::tests::scalar_subquery_resolves_per_execution_after_static_pruning) | test(=oracle::tests::datafusion_query_rejections_are_safe_and_actionable)'` (2 passed)
  - `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^oracle::codec::/)'` (4 passed)
  - `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::live_subquery::remote_two_row_scalar_subquery_is_refused) | test(=peer_network::live_subquery::remote_scalar_subquery_ships_as_a_literal) | test(=peer_network::live_subquery::remote_live_join_returns_its_rows)'"` (3 passed)
  - `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=query_coverage::common_queries_answer_over_published_and_live_rows)'"` (1 passed)
  - `mise run fmt`, `mise run lints`, `mise run codegen:check`, `mise run check:deps`, `mise run check:tenant-isolation`
  - `git diff --check 526355fb8c58dae6d67d516f98dddcd5c02b0e79`

Non-goals held: no Wyrd guard, SQL inspection, allowlist or second bound
implementation; no new proof type, node, protocol, resource owner or flag
change; `single_value`, rewrite, codec and execution untouched. Limits: the
correction removes the identified false proofs and overflow; it does not make
an unsound future `max_rows` branch unrepresentable. FULL JOIN of two
non-empty bounded sides is now unknown rather than a tighter sound bound
(`max(l*r, l+r)`), so such inputs are always validated at runtime.
