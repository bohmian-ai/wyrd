# bifrost-errors-R4 — require invariant filter values for singleton proof

Status: READY for `$wyrd-implement`.
Remediates: **FIND-bifrost-errors-3**; parent: the R3 scalar row-bound remediation.

## Subject and authority

- Approved spec: `changes/active/bifrost-errors/spec.md` (revision 1).
- Original task: none existed; the spec, owner instructions and R1–R3 packets
  govern this cumulative implementation.
- Review: `changes/active/bifrost-errors/review/task-review-4-resumed/verdict.md`
  and its independently validated `findings-validation.md`.
- Wyrd candidate: `17697dc8c2740213565e5f913cde1edfb0cb8ce5`.
- Integration baseline: `945a1548c47897dc94583169a6ba04163a73a749`;
  original task base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`;
  pre-merge task candidate: `7fe69545d0d8a2ab2a1e3033f52d5091e80d5b0e`.
- DataFusion candidate: `b2d37bc182b9d3e576650a414b364f3d9b5aefe3`;
  original fork base: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`.
- Distributed fork unchanged: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

Read repository authority and the validated ledger before implementation.
The owner requires common-query correctness, safe construction of proof state,
preservation of valid queries and no benchmark runs while other worktrees run.

## Diagnosis

REQ-006 and the prior scalar remediation require an evaluated scalar input with
more than one row to refuse, in both filter and projection positions. Runtime
validation may be skipped only with a guaranteed upper bound of one row.

In the pinned fork, `datafusion/expr/src/logical_plan/plan.rs:2909–2969`,
`Filter::is_scalar` collects unique-key equalities whenever the other operand
is not a bare Column. This does not establish that the comparison value is
fixed across the filter's input. A function, arithmetic or cast around another
input column still varies by row. GROUP BY correctly supplies a unique-key
dependency, but the filter misuses it to manufacture `max_rows = Some(1)`.

Concrete surviving input: `(x,n)=(1,1),(2,2)` from grouped counts below.

```sql
SELECT (SELECT x
        FROM (SELECT x, COUNT(*) n
              FROM (VALUES (1), (2), (2)) t(x)
              GROUP BY x) g
        WHERE x = abs(n)) AS picked
FROM (VALUES (0)) o(z);
```

The aggregate-dependent predicate survives optimization. The shared
`scalar_subquery_to_join.rs:383–405` build_join, used by filter and projection
rewrites, trusts the false bound and omits single_value. A traced flag-off
reproduction succeeded where an error was expected; the direct single_value
control refused. The source/log/provenance are recorded in the domain report
and independently checked in findings-validation.md. Replace the outer VALUES
with an existing authorized nonempty Bifrost table to reach the same path
through the public Analytical surface. The SELECT floor permits that shape.

The sibling `logical_plan/invariants.rs:188` also trusts this proof for
nonaggregated correlated scalars. A literal-only replacement would break
existing valid primary-key lookup against an outer reference, pinned by
`subquery.slt:439` (`non_aggregated_correlated_scalar_subquery_unique`).
That value is fixed during each inner invocation even though outer rows differ.

## Selected correction

Repair **the existing Filter::is_scalar proof producer in DataFusion**.
A key equality may contribute to singleton proof only when the opposing value
is positively proven fixed for the complete filter invocation, in either operand
order. Preserve fixed literals, bound parameters and typed OuterReferenceColumn
values; ordinary pure native operators/casts require all operands fixed, and
immutable/stable scalar calls require fixed arguments and their native volatility
contract. Preserve ordinary fixed arithmetic, cast and immutable-function
compositions of outer references.

Input Column dependence, per-row volatility and uncertain/runtime/opaque
scalar-variable, nested-plan, aggregate, window or higher-order forms cannot
contribute proof without an existing sound invariance contract. Reuse native
expression contracts and ordinary constant folding. Do not evaluate expressions
or recursively invoke optimization from the bound owner. Separate no-column
and nonvolatile checks are insufficient: native volatility traversal does not
prove every opaque expression form fixed. Preserve conjunction, complete
composite-key coverage and nullable-key semantics. The existing input-bound
fallback remains valid when no additional filter proof is available.

This closes the diagnosed class at its source: a row-dependent value cannot
produce the singleton proof that suppresses validation. Unknown bounds already
cause the native shared rewrite to add single_value. Retain that accumulator,
partial/final row counting, coalesce boundary, registry and codec. No Wyrd
SQL-shape guards, repeated consumer checks, new proof type, constancy framework,
parameter store, evaluator, dependency, node or protocol are justified.

Advance the existing Wyrd fork pin, lock file and ORACLE_DATAFUSION_REVISION
together. Extend the existing focused tests and peer journey, reusing their
client and fixtures. No new testing harness is needed.

## Constraints and preserved behavior

Preserve direct and compound valid correlated lookups, run_id correlation,
folded-constant and genuine literal-key proofs, normal parameter substitution,
MAX and LIMIT/fetch singleton optimizations. Retain FULL JOIN/GROUPING SETS
conservative bounds, checked arithmetic, empty/NULL/equal-row scalar behavior,
global cross-worker counting and native codec refusal.

Keep the distributed physical-scalar flag disabled, two optimizer passes,
hash-build readiness, first-poll snapshot, static pruning, disjoint planned
route ownership and literal/footer transfer proof. Keep current authorization,
error mapping, native query support and compact common-query corpus. Do not
change timeouts, retries, assertions or verification gates to obtain green.

No public API, error-status decision, remote protocol, generalized query
generator, benchmark or unrelated optimizer redesign is in scope. This repair
prevents the demonstrated unsafe construction; it does not claim a type-system
guarantee against every possible future optimizer defect.

## Acceptance criteria — all map to FIND-bifrost-errors-3

1. Native Filter/max_rows tests prove input-dependent arithmetic, function and
   cast operands cannot manufacture Some(1) over two surviving unique-key
   groups. Both equality orders, composite completeness and nullable-key rules
   are covered with positive and negative controls.
2. Volatile or unresolved expression forms cannot grant the proof; test the
   proof deterministically rather than asserting probabilistic random output.
   Literal/folded constants, bound parameters and genuine singleton controls
   retain their sound behavior.
3. Existing unique correlated primary-key SQL lookup remains accepted and
   correct; fixed outer-reference arithmetic/cast/immutable-function controls
   remain accepted. Expressions dependent on inner rows remain unproven.
4. With physical uncorrelated scalar execution disabled, native filter and
   projection forms of the concrete count/abs case refuse multiple rows.
   Include native physical-scalar reference and single_value controls.
5. The existing Analytical public negative peer journey refuses those filter
   and projection forms against a real authorized outer table, returning the
   existing execution-failure contract. Existing literal/footer, remote join,
   corpus and mapper checks remain green.
6. Fork pin/lock/codec revision agree; focused changed-owner checks pass with
   recorded exact selectors, counts and exit codes. Earlier scalar controls,
   partial/final merge and codec remain green.

## Focused verification

Record red then green at the native proof owner and flag-off SQL reproduction.
Run the native changed-owner tests and subquery SQL file using the fork's mise
toolchain; cargo-nextest was unavailable there, so use exact `cargo test`
selectors for named unit tests. Record actual selectors for added tests and the
SQL runner's exact one-file selection, ensuring nonzero selected counts.
Retain the existing correlated SQL case in that file. Run fork formatting and
changed-crate clippy. Do not rerun all 520 SQL files or broad journey aggregates
as a remediation requirement.

Use these existing focused Wyrd selectors with the updated fork:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::live::tests::scalar_subquery_resolves_per_execution_after_static_pruning) | test(=oracle::tests::datafusion_query_rejections_are_safe_and_actionable)'

mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(/^oracle::codec::/)'

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::live_subquery::remote_two_row_scalar_subquery_is_refused) | test(=peer_network::live_subquery::remote_scalar_subquery_ships_as_a_literal) | test(=peer_network::live_subquery::remote_live_join_returns_its_rows)'"

mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=query_coverage::common_queries_answer_over_published_and_live_rows)'"

mise run fmt
mise run lints
mise run codegen:check
mise run check:deps
mise run check:tenant-isolation
git diff --check
```

Reconfirm selectors against current source before execution. Serialize shared
Cargo verification lanes. Diagnose failures with tracing under AGENTS rather
than changing retries or timeouts. No synthetic load, raised concurrency or
benchmarks. Full capability/change verification belongs to change review once.
Record any verification limitation explicitly; do not describe unexecuted
checks as passing.

## Implementation evidence

Status: IMPLEMENTED. DataFusion fork `wyrd-production` at
`8c3f18e9806ae23e8840a710af775c058dfad66a` (pushed); distributed fork
unchanged at `8fac2a978…`. Wyrd `121b33567` advances pins, lock and
`ORACLE_DATAFUSION_REVISION` together and extends the existing journey.

Owner and gap: `Filter::is_scalar` (`datafusion/expr/src/logical_plan/plan.rs`)
is the only producer of the filter singleton proof that `max_rows` returns as
`Some(1)`; `ScalarSubqueryToJoin::build_join` and `logical_plan/invariants.rs`
consume it. It accepted any non-Column operand as the key's value. The fix
replaces that arm with one guard, `is_fixed_value`, for either operand order.
It accepts only literals, placeholders, outer references, and
Alias/Cast/TryCast/Negative/Not/BinaryExpr/ScalarFunction whose operands are
all fixed. A ScalarFunction also needs non-`Volatile` native signature
volatility. Everything else is unproven, including input columns,
ScalarVariable, subqueries, aggregates, windows and lambdas.
Conjunction splitting, composite-key coverage, nullable-key exclusion and the
input-bound fallback are unchanged. There is no evaluation, optimizer
recursion, new type or Wyrd guard; `single_value`, the rewrite, the codec and
execution are untouched.

Red: on the old proof, `filter_is_scalar_only_for_fixed_key_values` failed at
`a = b + Int64(0)` (true, expected false; every fixed control passed). The
flag-off `subquery.slt` filter and projection count/abs cases succeeded. On
the old pin, `remote_two_row_scalar_subquery_is_refused` returned 400 rows for
the count/abs projection. Green: all pass. The traced journey
(`WYRD_LOG=info,vala_bifrost_redux=debug`) logs `Scalar subquery returned more
than one row` for all six refusal queries. The client receives
`WYRD_VALA_500_QUERY_EXECUTION_FAILED`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1 Input-dependent arithmetic/function/cast cannot prove Some(1); both orders, composite, nullable controls | `is_fixed_value` guard in `Filter::is_scalar` | `filter_is_scalar_only_for_fixed_key_values`: `b + 0`, `CAST(b)`, `f_immutable(b)` false in both orders; composite `a=1 AND b=2` true, `a=1` / `b=c` / `b=c*2` false; nullable UNIQUE false; grouped count `column1 = n + 0` keeps input bound `Some(3)`, `= 2` gives `Some(1)` | PASS |
| 2 Volatile/unresolved forms cannot grant proof; literals, folded constants, parameters, singletons keep proof | volatility from native `signature().volatility`; unknown variants fall to `false` | same test: volatile call and ScalarVariable false (decided structurally, nothing evaluated); literal, `$1`, stable call over literal true; slt fixed-key `k = abs(-2)` returns `2`; existing MAX/LIMIT slt cases | PASS |
| 3 Correlated PK lookup accepted; fixed outer-ref arithmetic/cast/immutable function accepted; inner-row dependence unproven | `OuterReferenceColumn` is fixed, composed recursively | same test: `outer`, `outer + 1`, `CAST(outer)`, `f(outer)` true; `subquery.slt:439` `non_aggregated_correlated_scalar_subquery_unique` passes | PASS |
| 4 Flag-off filter and projection count/abs refuse; physical reference and single_value controls | proof correction only | `subquery.slt` flag-off filter and projection (Red→Green), physical-path reference and direct `single_value` control (refuse before and after) | PASS |
| 5 Analytical public journey refuses filter and projection against a real authorized table; literal/footer, remote join, corpus, mapper green | count/abs projection and `WHERE id > …` filter added to the existing journey over the registered events table | the three `peer_network::live_subquery` journeys; `query_coverage::common_queries_answer_over_published_and_live_rows`; mapper unit | PASS |
| 6 Pin/lock/codec agree; focused checks recorded; earlier scalar, merge, codec green | `Cargo.toml`, `Cargo.lock`, `oracle/codec.rs` at `8c3f18e98` | `cargo metadata --locked`; codec unit tests; fork proto and `single_value` tests | PASS |

Commands (all exit 0 unless noted as Red):

- Fork (`~/Documents/GitHub/datafusion-scalar-snapshot`; `cargo-nextest` is unavailable there):
  - `cargo test -p datafusion-expr --lib logical_plan::plan::tests::filter_is_scalar_only_for_fixed_key_values -- --exact` (1 passed; Red before the fix)
  - `cargo test -p datafusion-sqllogictest --test sqllogictests -- subquery.slt` (1 file; Red with the old `plan.rs`, 2 errors)
  - `cargo test -p datafusion-expr --lib` (265), `cargo test -p datafusion-optimizer --lib` (886), `cargo test -p datafusion-proto --test proto_integration` (265), `cargo test -p datafusion-functions-aggregate --lib single_value` (4)
  - `cargo test -p datafusion-sqllogictest --test sqllogictests` (520/520; not required, run because the proof feeds other rules)
  - `cargo fmt --all -- --check`; `cargo clippy -p datafusion-expr --all-targets --all-features -- -D warnings`
- Wyrd: the four focused selectors exactly as listed under Focused verification
  (2, 4, 3 and 1 tests passed); `mise run fmt`, `lints`, `codegen:check`,
  `check:deps`, `check:tenant-isolation`; `git diff --check 526355fb8c58dae6d67d516f98dddcd5c02b0e79`.

Non-goals held: no Wyrd SQL-shape guard, constancy framework, evaluator, new
proof type, dependency, node, protocol, flag, timeout or retry change; no
benchmark or synthetic load. Limits: `CASE`, `BETWEEN`, `IS NULL`-style and
other expression forms are now conservatively unproven. A query that relied on
them for a singleton proof gets runtime `single_value` validation instead of
the bypass; results are unchanged. Stable functions count as fixed under their
native per-query contract.
