# bifrost-errors-R5 — preserve fixed correlated scalar expressions

Status: READY for `$wyrd-implement`.
Remediates: **FIND-bifrost-errors-5** (REGRESSION).
Parent: R4 fixed-filter-value remediation; original FIND-3 unsafe-bound defect
is closed on this candidate.

## Authority and immutable inputs

- Spec: `changes/active/bifrost-errors/spec.md`, approved revision 1.
- Original task: none existed; spec, owner directions and R1–R4 packets govern.
- Review and independent ledger: `changes/active/bifrost-errors/review/task-review-5/`.
- Wyrd candidate: `8a54d81c346db464b624e81ebe8958c63bb7e228`.
- Integration baseline: `945a1548c47897dc94583169a6ba04163a73a749`;
  original task base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`;
  previous reviewed candidate: `17697dc8c2740213565e5f913cde1edfb0cb8ce5`.
- DataFusion cumulative base: `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`;
  current: `8c3f18e9806ae23e8840a710af775c058dfad66a`;
  prior fork: `b2d37bc182b9d3e576650a414b364f3d9b5aefe3`.
- Distributed fork unchanged: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

Read governing repository/native authority and the validated ledger. Owner
requires valid common/correlated queries preserved, safe proof construction
at its native owner, compact product tests and no benchmarks while other
worktrees run. This is a bounded correction, not another architecture plan.

## Diagnosis and impact

REQ-006 and R4 require preserving valid correlated lookups while preventing
row-dependent expressions from falsely proving singleton cardinality.

The current native `Filter::is_scalar` correctly checks the opposing key value
using a positive classifier (`expr/src/logical_plan/plan.rs:2963–2987`).
But ordinary pure CASE falls into its unproven default even when it contains
only outer references/literals. That loses a valid <=1 proof on a unique key.
The sibling correlated scalar checker (`logical_plan/invariants.rs:188–194`)
uses that proof for admission and refuses unknown bounds. It does not reach
the uncorrelated rewrite's runtime single_value fallback. The implementation
risk statement that excluded forms only add runtime validation is therefore
incorrect for this consumer.

This valid query has one comparison value per inner invocation:

```sql
SELECT o.id,
       (SELECT g.id
        FROM (SELECT i.id, COUNT(*) n
              FROM vala.bifrost.events i GROUP BY i.id) g
        WHERE g.id = CASE WHEN o.id > 0 THEN o.id ELSE -o.id END) AS picked
FROM vala.bifrost.events o;
```

Use a registered required nonnullable id. Native GROUP BY creates the unique
dependency; the alias preserves it, and Filter-over-alias reaches the generic
correlated bound check. No Bifrost PRIMARY KEY DDL feature is required. Current
SELECT/table admission accepts this shape before physical topology selection.

The focused native fixture in `/tmp/bifrost-r5-correlated-proof` selected 1/1
files: direct PK and nonnullable grouped-key controls pass, equivalent CASE
forms fail analysis with `Correlated scalar subquery must be aggregated to
return at most one row`. Expected native PK rows are `(11,3),(22,1),(33,NULL),
(44,3)`, including an unmatched outer row. Source delta proves the old
non-Column arm admitted this sound expression; no old checkout/run was used.
Read behavior/validation reports for exact log and binary-provenance limits.
This is a newly refused valid query, distinct from the now-closed wrong-result
cardinality bypass, but it shares the same proof owner.

## Selected smallest correction

Extend **the existing native Filter proof producer** to preserve ordinary pure
scalar composition when **every value-bearing child is invocation-fixed**.
Retain the closed positive default, existing fixed leaves, arithmetic/casts,
and native stable/immutable-function contract. Cover the following existing
native family together, rather than adding one CASE exception:

| Native pure family | Required fixed operands |
|---|---|
| CASE | Optional base, every WHEN/THEN component, optional ELSE |
| BETWEEN | Tested value and both bounds |
| IN-list | Tested value and all items |
| LIKE/SIMILAR TO | Value and pattern; flags/escape metadata stay static |
| Null and truth tests | Operand of IsNull/IsNotNull, IsTrue/IsFalse/IsUnknown and IsNotTrue/IsNotFalse/IsNotUnknown |

Native `Expr::apply_children` (`expr/src/tree_node.rs:45–105`) already exposes
all these children; reuse native traversal where it avoids omission while
keeping expression eligibility positive. The native constant evaluator
recognizes the same ordinary pure family, but has a different binding/context
contract and is not a drop-in outer-invocation proof. Do not evaluate expressions
or recursively optimize from max_rows, and do not create a constancy framework.

Any input Column, volatile descendant, scalar variable, subplan, aggregate,
window, grouping/unnest or higher-order/lambda binding remains unproven.
Unknown/new variants must not acquire proof by default. Even an unused branch
or condition must not hide an unsafe descendant. Preserve composite-key
completeness, nullable-key rules, conjunction semantics and input-bound fallback.

This repair prevents the demonstrated invalid proof from being constructed
while preserving the valid proofs required by both consumers. Do not weaken
correlated admission, restore the old non-Column rule, add runtime correlated
validation, or patch query shapes in Wyrd. No new proof type, evaluator,
parameter store, dependency, node or protocol is needed. Keep single_value,
its native partial/final state and codec, shared rewrite and runtime unchanged.
Advance Wyrd fork pin, lock and ORACLE_DATAFUSION_REVISION together.

## Why the repeated failures and what stops this round

The earlier FULL JOIN/grouping and row-dependent Filter failures shared an
unsound guaranteed-bound authority. The latest positive classifier fixes that
unsafe side, but omits sound ordinary expressions needed by a sibling admission
consumer. Existing tests covered selected positive forms and uncorrelated
runtime fallback; they did not exercise this shared contract through correlated
analysis. Repair/test the pure family and both consumer consequences together.
Do not expand this into an exhaustive generated workload or claim every future
optimizer bug is impossible. The original error mapping and remote predicate
loss were separate boundaries and require no replacement here.

## Preserved behavior and non-goals

Keep count/abs and all input-dependent/volatile refusal proofs, FULL JOIN/
GROUPING SETS/checked-overflow bounds, equal/NULL/empty scalar semantics,
globally merged native row counts and codec refusal. Preserve direct/compound
outer references, bound parameters, folded constants, MAX and LIMIT singleton
optimizations. Keep existing authorization and caller-error mapping.

Retain disabled distributed physical-scalar flag, two optimizer passes, native
hash-build readiness, first-poll snapshot, static pruning, planned route
ownership and exact remote literal/footer evidence. No query timeout, retry,
assertion, gate, public error contract or lifecycle/resource owner changes.
No benchmark, synthetic load, raised concurrency, broad generated query
framework or unrelated optimizer redesign.

## Acceptance criteria — FIND-bifrost-errors-5

1. Proof-owner tests cover every pure family listed above in both equality
   orders, with fixed outer/literal positives and inner-column/volatile
   descendants negative. CASE includes searched/simple forms, optional base/
   ELSE, condition and result dependencies. Assertions are deterministic.
2. Existing native direct unique-key correlation remains green; CASE PK and
   nonnullable grouped-derived correlated lookups return exact rows, including
   an unmatched row. Representative pure sibling SQL cases verify the same
   analyzer consumer, rather than only calling the classifier.
3. An existing public Bifrost journey/corpus includes the grouped-key correlated
   CASE query and expected rows using a required id, reusing its client/data.
   No new harness, query oracle or production testing dependency is added.
4. Count/abs filter/projection refusal, physical reference, direct single_value,
   bound/merge/codec controls and remote literal/footer/route checks remain
   green. The formerly wrong-result cases cannot regain a singleton bypass.
5. Fork/pin/lock/codec identities agree and focused changed-owner checks pass,
   with exact nonzero selectors/counts, exit codes and red/green evidence.

## Focused verification

Start with the CASE correlated regression and then prove pure-family sibling
coverage and unsafe descendants at the existing native proof owner. Named
native tests use exact selectors through the mise toolchain; cargo-nextest was
unavailable in that fork, so use its cargo test fallback. Existing focused
commands from the fork root:

```bash
mise exec -- cargo test -p datafusion-expr --lib \
  logical_plan::plan::tests::filter_is_scalar_only_for_fixed_key_values -- --exact

mise exec -- cargo test -p datafusion-sqllogictest --test sqllogictests -- subquery.slt
```

Record the exact names/selectors for any added tests and selected SQL-file
count. Retain prior bound, native partial/final and codec controls in the
narrowest changed-owner lanes; record actual selectors rather than inventing
unverified names. Run fork fmt and changed-crate clippy. No all-520-file SQL
rerun or broad journey aggregate is required by this remediation.

Existing Wyrd focused selectors, including the corpus if extended:

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

If another existing journey owns the added positive case, record and run its
exact selector with repository-managed setup. Reconfirm commands against the
current code and serialize shared Cargo lanes. Diagnose failures with tracing;
do not alter timeouts or retries. Full capability/change verification runs once
at change review. State limitations honestly; no unexecuted check is green.

## Implementation evidence (R5)

Fork `wyrd-production` commit `b8ec4b1d291cd8e371bb88fc76959d91e8503a1d`
(pushed; parent `8c3f18e98`). Wyrd commit `ed1626ad4` advances `Cargo.toml`
(11 entries), `Cargo.lock` (35 sources) and `ORACLE_DATAFUSION_REVISION`
together to that SHA.

Owner and gap: `Filter::is_scalar`'s `is_fixed_value`
(`datafusion/expr/src/logical_plan/plan.rs`) had a closed positive list
without CASE, BETWEEN, IN, LIKE/SIMILAR TO or null/truth tests. The list now
names those variants beside the existing operators, casts and functions, and
proves each one through native `Expr::apply_children`. Every child must be
fixed, so CASE bases, conditions, unused branches and ELSE are all checked.
A volatile scalar function is refused before its children are traversed.
Leaves are unchanged, and every unlisted variant is still `false`.
`invariants.rs`, the rewrite, `single_value`, the codec and the Wyrd query
path are unchanged.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1. Pure-family proof tests | `is_fixed_value`; test `filter_is_scalar_only_for_fixed_key_values` extends the leaf table with 23 shapes, each placing a probe in one child slot. Shapes: searched CASE condition, THEN, ELSE and no-ELSE; simple CASE base and WHEN value; three BETWEEN slots; two IN slots, one negated; LIKE and SIMILAR TO, value and pattern; and the 8 null/truth tests. Each takes outer and literal probes (true) and inner-column and volatile probes (false), in both equality orders. | Red: `a = CASE WHEN outer_ref(o.x) > Int64(0) THEN outer_ref(o.x) ELSE Int64(0) END` (left false, right true). Green: `mise exec -- cargo test -p datafusion-expr --lib logical_plan::plan::tests::filter_is_scalar_only_for_fixed_key_values -- --exact` gives 1 passed, exit 0. | PASS |
| 2. Correlated analyzer consumer | `subquery.slt` after `#non_aggregated_correlated_scalar_subquery_unique`. Positives: a PK CASE lookup, a grouped `t3_id` CASE lookup, and a PK lookup whose CASE condition combines BETWEEN, IN, LIKE, SIMILAR TO, IS NOT NULL and IS TRUE. All return exact rows, including `33 NULL`. Negatives: a CASE on inner `g.n` and a CASE with `random()` are refused with "must be aggregated". The direct PK lookup is unchanged. | Red: subquery.slt failed only at lines 450, 458 and 466 (the three positives, "Correlated scalar subquery must be aggregated"). Green: `mise exec -- cargo test -p datafusion-sqllogictest --test sqllogictests -- subquery.slt` completed 1/1 files, exit 0. | PASS |
| 3. Public Bifrost journey | `remote_live_join_returns_its_rows` (`live_subquery.rs`) reuses the seed and one public client. It adds the grouped required-`id` CASE lookup over `events` for ids 107 and 199, expecting `(107, Some(108))` and `(199, None)`. | Red on pin `8c3f18e98`: `QueryInvalidSql { "Error during planning: Correlated scalar subquery must be aggregated to return at most one row" }`. Green on `b8ec4b1d2`: the three live-subquery selectors gave 3 passed, exit 0. | PASS |
| 4. Refusal and control preservation | Count/abs and FULL JOIN/GROUPING SETS cases are unchanged. A new flag-off row-dependent case `k = CASE WHEN n > 0 THEN n END` is refused at runtime. | subquery.slt green; `remote_two_row_scalar_subquery_is_refused` and `remote_scalar_subquery_ships_as_a_literal` pass. Fork suites: expr lib 265, optimizer lib 886, functions-aggregate `single_value` 4 and proto lib 17, all passing. Wyrd: the 2 named `vala-bifrost-redux` unit tests pass, and `oracle::codec::` gives 4 passed. | PASS |
| 5. Identities and checks | Pin, lock and codec agree on `b8ec4b1d2`; `cargo metadata --locked` exits 0. | Fork: `cargo fmt --all -- --check` 0; `cargo clippy -p datafusion-expr --all-targets --all-features -- -D warnings` 0. Wyrd: corpus `query_coverage::common_queries_answer_over_published_and_live_rows` 1 passed; `fmt`, `lints`, `codegen:check`, `check:deps` and `check:tenant-isolation` all 0; `git diff --check 526355fb8 HEAD` 0. | PASS |

Diagnosis of the unexpected first red run:
- **Symptom:** the first red run failed with `duplicate key value violates unique constraint "cards_identity_unique"`.
- **Evidence:** a stack trace at `live_subquery.rs:327`.
- **Cause:** the extended journey called the `query` helper twice. Each call provisions an API key Card under the same name, `live-subquery`.
- **Fix site:** the journey now provisions once and reuses one `Bifrost` client, as `remote_two_row_scalar_subquery_is_refused` already does. The helper and its other caller are unchanged.

Non-goals: the correlated analyzer, the rewrite, `single_value` state, the
codec format, the distributed fork, timeouts, retries and error mapping are
all unchanged. There was no benchmark, no all-520 SQL rerun and no broad
journey aggregate.

Limits:
- Stable functions still count as fixed, per the native volatility contract.
- Opaque forms such as subqueries, aggregates, windows, lambdas, unnest and
  scalar variables remain unproven by design.
- Scalar subquery refusal is asserted through the public error code; traced
  log counts were not captured on passing nextest runs.
