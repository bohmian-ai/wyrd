---
id: bifrost-errors-R2
kind: remediation
status: ready
spec: changes/active/bifrost-errors/spec.md
spec_revision: 1
requirements: [REQ-006, INV-001, AC-003]
parent_task: bifrost-errors-R1
remediates: [FIND-bifrost-errors-3, FIND-bifrost-errors-4]
---

# Enforce analytical scalar cardinality at its DataFusion producer

## Subject and authority

- Repository: `/home/thorrester/Documents/GitHub/wyrd-worktrees/split/bifrost-errors`.
- Approved spec: `changes/active/bifrost-errors/spec.md`, revision 1.
- Original task: none; approved spec and owner directions govern.
- Reviewed base: `526355fb8c58dae6d67d516f98dddcd5c02b0e79`.
- Reviewed candidate: `7d6a1ff95b3dc09315e3af236ab9906b305e11d0`.
- Prior remediation: `review/task-review-1/bifrost-errors-R1-materialize-live-subquery-predicates.md`.
- Reviewed DataFusion: `5697cfa35192f175bbcd54f59a5fd88527c2bbf5`.
- Reviewed distributed fork: `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.
- Verdict: `review/task-review-2/verdict.md`.
- Independent validated ledger: `review/task-review-2/findings-validation.md`.

Route directly to `$wyrd-implement`. Review is complete; implementation starts
from the immutable candidate and must revalidate current fork owners/callers.

## Diagnosis

### FIND-bifrost-errors-3: a scalar is executed with unrestricted join semantics

REQ-006 accepts scalar-subquery behavior. An executed scalar returns one value,
typed NULL for no rows, or fails if its input produces multiple rows. The
analytical planning session installs the distributed builder, which disables
physical uncorrelated scalar execution. DataFusion's legacy
`ScalarSubqueryToJoin` then joins the aliased scalar input without enforcing
its cardinality (`optimizer/src/scalar_subquery_to_join.rs:108-238,339-430`,
particularly 389-391). Filter and Projection share that producer. Its config
documentation (`common/src/config.rs:1626-1640`) explicitly warns that
multiple rows can produce successful wrong results.

Using the compact fixture, the inner query below returns amounts 50 and 30:

```sql
SELECT order_id
FROM vala.datasets.coverage_orders
WHERE amount = (
    SELECT amount
    FROM vala.datasets.coverage_orders
    WHERE order_id IN (10, 11)
)
ORDER BY order_id
```

Analytical execution treats those rows as a set to join, permitting matches
to either value. Scalar execution should refuse the second row. A projection
scalar can instead multiply outer rows. The flag predates this task, but this
is the scalar capability being accepted, not unrelated engine debt.

The two-pass optimizer correction improves equality planning and the hash
join's filter, but cannot restore scalar semantics. The retained join is
itself applying the wrong set semantics. The existing corpus qualifies
singleton, empty and NULL input, omitting multiple-row refusal; all its cases
can therefore pass with this defect present.

The existing semantic owner is DataFusion's complete scalar extraction path in
`physical-plan/src/scalar_subquery.rs:397-436`. It reads the complete stream,
rejects a second row even in a later batch, retains duplicate-row multiplicity,
and synthesizes typed NULL on empty input. `execute_stream` coalesces multiple
partitions. The analytical rewrite bypasses this owner.

### FIND-bifrost-errors-4: two changed test docs omit mandatory panic conditions

AGENTS §16 and the Rust documentation authority require Panics sections where
panics remain possible. The materially rewritten mapper unit test at
`oracle/mod.rs:4473-4477` and expanded MCP error journey at
`wyrd-mcp/tests/bifrost/mcp/query.rs:202-214` document intent but omit their
assertion panic conditions. Correct these in the semantic implementation
round; do not create a separate style round or change test behavior.

## Intended outcome and smallest correction

Keep the distributed physical-scalar flag disabled. Extend the owned
DataFusion scalar-to-join producer so an uncorrelated scalar input is a native
relational scalar dependency that validates its whole input before emitting
its scalar row. Reuse the existing scalar extractor's zero/one/multiple-row
semantics. Emit exactly one typed scalar row, including typed NULL for empty
input, or a typed execution error on a second row. Apply this at the shared
rewrite producer used by Filter and Projection, before either hash or no-key
join consumers can observe that input.

A suitable installed relational cardinality assertion or single-value
aggregate is absent in the reviewed fork. Merely re-enabling
ScalarSubqueryExec is unsafe because distributed stage cuts can separate its
result container from dependent expressions. A post-join check is too late:
it can miss invalid inner multiplicity when no outer rows match and can
mistake valid outer multiplicity for multiple scalar input rows.

Use DataFusion's existing global single-partition distribution/coalescing
contract for this validation. Independent worker-local checks are insufficient:
two workers can each observe one row while the scalar has two globally. The
distributed planner already handles CoalescePartitionsExec using
NetworkCoalesceExec and Maximum(1)
(`inject_network_boundaries.rs:364-400`). Keep validation above that existing
collection boundary. Any necessary native relational node uses DataFusion's
ordinary physical-plan proto encode/decode ownership and relational child
contracts. This avoids a process-local expression-results scope, a new stage
protocol, and a Wyrd evaluator.

Advance Wyrd's DataFusion pins, lock entries and codec revision fence together
through the normal dependency path. Qualify the existing distributed collection
and codec path; extend that fork only if ordinary native-node integration
requires a bounded change, without inventing new scheduling or resource
semantics. If existing contracts cannot establish global cardinality without
a material new stage/resource decision, return SPEC_REVISION_REQUIRED rather
than weakening validation.

Preserve the existing R1 snapshot, lazy first-poll timing, two-pass optimizer,
native equality runtime filter and compact corpus. Add the two concise Panics
sections, and clean up the prior verdict's trailing blank line identified by
the cumulative diff-check.

## Constraints and non-goals

- Preserve typed NULL, singleton types/metadata and duplicate-row cardinality.
- Preserve existing correlated analyzer/decorrelation behavior; this is not a
  general decorrelation rewrite.
- Preserve the authoritative join/filter, static pruning, disjoint planned
  route assignment, tenant binding, fingerprint and peer admission.
- Preserve existing query memory/spill ownership, deadline, cancellation,
  failure mapping and lifecycle; no new resource owner or retry/fallback path.
- Do not use DISTINCT, LIMIT 1 or first-value aggregation to hide extra rows.
- Do not add Wyrd SQL inspection, expression allowlists, scalar evaluation,
  public contracts, generated workload frameworks or result normalization.
- Do not require analytical range-filter optimization: it is a documented
  existing limit, not a retained finding in this review.
- Enforce cardinality whenever the scalar dependency is evaluated. Do not
  require evaluation of work an optimizer legitimately eliminates; use the
  existing native scalar path as the reference for empty-outer behavior.
- No benchmarks, synthetic load, raised concurrency or timing thresholds.
- Do not diagnose the earlier untraced reservation observation from a green
  rerun; if a relevant failure recurs, rerun with tracing and diagnose it.

## Acceptance criteria

1. FIND-3: analytical-configured execution refuses two scalar rows rather than
   returning successful set-join results, for both Filter and Projection.
2. FIND-3: validation is global across batches and partitions, counts duplicate
   values as multiple rows, and happens before the evaluated scalar dependency
   emits its row. Invalid scalar input is not hidden by no matching outer rows.
3. FIND-3: empty, typed-NULL and singleton results retain native scalar behavior;
   compare empty-outer behavior with the existing scalar execution reference.
4. FIND-3: a real SDK/server negative journey uses analytical configuration and
   proves refusal when scalar rows span published/live inputs. Native codec
   roundtrip and the existing network-coalesce path preserve global validation.
5. FIND-3: existing equality literal/footer proof, representative remote join,
   common-query corpus, error mapping and tenant/redaction behavior remain
   unchanged. No flag reversal or new stage/resource protocol is introduced.
6. FIND-4: both changed test items have substantive Panics documentation; test
   assertions and behavior remain intact. Cumulative diff-check is clean.

## Focused proof and execution discipline

Use Red-Green-Refactor for the multi-row scalar failure: first prove the wrong
successful result on this candidate through the current analytical path, then
implement the native producer correction and demonstrate refusal. Add only
small fixed cases for the global, duplicate, null, empty and projection seams.
Record and run each added test's exact package, target and selector through the
fork's canonical toolchain and Wyrd's `mise exec -- cargo nextest run --locked`.
Use repository-managed Postgres setup for Wyrd journeys. Do not use positional
filters that can select no tests. The documentation correction uses source
inspection and formatting, not a new test.

Retain the existing exact regression commands:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  -E 'test(=oracle::live::tests::scalar_subquery_resolves_per_execution_after_static_pruning) | test(=oracle::tests::datafusion_query_rejections_are_safe_and_actionable)'
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::live_subquery::remote_scalar_subquery_ships_as_a_literal) | test(=peer_network::live_subquery::remote_live_join_returns_its_rows)'"
mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:all:inner && cargo nextest run --locked -p wyrd-ai --test integration -P journey --run-ignored=all -E 'test(=query_coverage::common_queries_answer_over_published_and_live_rows)'"
```

Run affected fork scalar/rewrite/physical-planning/codec tests with their exact
selectors and normal toolchain, including the new global collection proof.
Run Wyrd `mise run fmt`, `mise run lints`, `mise run codegen:check`,
`mise run check:deps`, and `mise run check:tenant-isolation` for the actual
write set. Run cumulative `git diff --check` from the original reviewed base
to the resulting candidate, not only a clean-working-tree check.

Do not require a whole journey suite or broad correctness aggregate in this
remediation. The complete Bifrost capability gate runs once at later integrated
change review. Keep Cargo-backed verification sequential across shared targets.

## Implementation evidence

Status: IMPLEMENTED. DataFusion fork `wyrd-production` at
`f059acf6c58b2c5fd4bc36b3358e8a56ac283476`; distributed fork unchanged at
`8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

Owner and gap: `ScalarSubqueryToJoin::build_join` is the one producer for
Filter and Projection; it joined uncorrelated scalar input unrestricted. The
fork had no cardinality-asserting relational operator. The extension is
Calcite's standard `SINGLE_VALUE` aggregate: `single_value` in
`functions-aggregate` (default registry, ordinary AggregateUDF proto
encoding), which the rule wraps around an uncorrelated subquery whose
`max_rows()` is not at most 1, fetched from the function registry as
`ReplaceDistinctWithAggregate` fetches `first_value`. Partial states carry the
row count; the no-group Final aggregate merges every partition above the
existing `CoalescePartitionsExec`, which datafusion-distributed already turns
into its network coalesce. No new node, stage, resource, or codec path.
Without a registered `single_value` the rule leaves such a subquery
unrewritten instead of joining it as a set. Aggregated singletons (for
example `max`) are not wrapped, so the R1 hash-join runtime filter is
unchanged.

Red: before the change, six new flag-off `subquery.slt` cases succeeded, and
`remote_two_row_scalar_subquery_is_refused` returned 2 rows on the Analytical
peer path. Green: trace shows `Oracle DataFusion operation failed
error=Execution error: Scalar subquery returned more than one row`; the client
receives `WYRD_VALA_500_QUERY_EXECUTION_FAILED`.

| Acceptance criterion | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| 1 Filter and Projection refuse two rows on the analytical configuration | fork `optimizer/src/scalar_subquery_to_join.rs` `build_join` wraps in `single_value`; `functions-aggregate/src/single_value.rs` | fork `subquery.slt` flag-off filter, `=`, projection, and arithmetic cases; Wyrd `peer_network::live_subquery::remote_two_row_scalar_subquery_is_refused` (filter and projection) | PASS |
| 2 Global across batches and partitions, duplicates count, validated before the scalar row emits, not hidden by no matching outer rows | accumulator counts rows across `update_batch` and `merge_batch`; Final above coalesce | `single_value::tests::*` (later batch, equal and NULL second rows, merged partials); slt EXPLAIN pins Partial→Coalesce→Final as the hash-join build side; slt `UNION ALL` duplicate and `x > 100 AND …` cases; corpus no-outer-row refusal | PASS |
| 3 Empty, typed NULL, singleton keep native behavior | empty input gives typed NULL; `return_field` keeps type and metadata | `no_rows_is_a_typed_null`, `one_row_is_its_value_even_when_null`; slt existing empty/NULL/singleton cases, new one-row-across-partitions and `arrow_typeof` cases | PASS |
| 4 Real SDK/server negative journey on analytical config, spanning published/live; codec roundtrip and network coalesce keep global validation | pair table: one published row and one live duplicate on the remote Scribe | Wyrd peer journey above; Rust SDK corpus refusals over published order 11 and live order 15; fork `cases::plans::scalar_subquery::roundtrip_joined_scalar_subquery_refuses_two_rows` | PASS |
| 5 Literal/footer proof, remote join, corpus, error mapping, tenancy unchanged; no flag reversal or new protocol | flag untouched; pins, lock, `ORACLE_DATAFUSION_REVISION` advanced together | `remote_scalar_subquery_ships_as_a_literal`, `remote_live_join_returns_its_rows`, `query_coverage::common_queries_answer_over_published_and_live_rows`, mapper unit, `check:tenant-isolation` | PASS |
| 6 Panics docs; tests unchanged; cumulative diff-check clean | `oracle/mod.rs` mapper test and `wyrd-mcp/tests/bifrost/mcp/query.rs` journey docs; task-review-1 verdict trailing line removed | source inspection; `git diff --check 526355fb8…` exit 0 | PASS |

Commands (all exit 0):

- Fork (`~/Documents/GitHub/datafusion-scalar-snapshot`, its pinned toolchain):
  `cargo test -p datafusion-sqllogictest --test sqllogictests` (520 files);
  `cargo test -p datafusion-optimizer --lib` (886);
  `cargo test -p datafusion-functions-aggregate --lib` (245, including
  `single_value::tests::*`); `cargo test -p datafusion-proto` (all targets);
  `cargo clippy -p datafusion-functions-aggregate -p datafusion-optimizer --all-targets -- -D warnings`;
  `cargo clippy -p datafusion-proto --tests -- -D warnings`;
  `cargo fmt --all -- --check`; `dev/update_function_docs.sh` and
  `dev/update_config_docs.sh` regenerated the docs.
- Wyrd:
  `mise exec -- scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && cargo nextest run --locked -p wyrd-testing --test oracle -P journey --run-ignored=all -E 'test(=peer_network::live_subquery::remote_two_row_scalar_subquery_is_refused) | test(=peer_network::live_subquery::remote_scalar_subquery_ships_as_a_literal) | test(=peer_network::live_subquery::remote_live_join_returns_its_rows)'"`;
  the R2 corpus command as written above; the R2 unit command as written
  above; `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib -E 'test(/^oracle::codec::/)'`;
  `mise run fmt`, `mise run lints`, `mise run codegen:check`,
  `mise run check:deps`, `mise run check:tenant-isolation`; cumulative
  `git diff --check 526355fb8c58dae6d67d516f98dddcd5c02b0e79`.

Diagnosis recorded during green: the projection refusal first failed with
`duplicate key value violates unique constraint "cards_identity_unique"`.
Cause: the journey's `query` helper provisions a public API key per call, and
the second provisioning reuses the same card identity. Fix site: the new
journey provisions one key and reuses one client; no assertion, timeout, or
retry changed.

Non-goals held: no flag reversal, DISTINCT/LIMIT/first-value masking, Wyrd SQL
inspection, scalar evaluator, public Wyrd contract, stage/resource protocol,
benchmark, or synthetic load. Fork docs that called flag-off multi-row results
silently wrong now describe the `single_value` check. Limits:
`single_value` is a SQL-callable default aggregate in the fork, as Calcite's
`SINGLE_VALUE` is; the refusal surfaces as the existing opaque
`WYRD_VALA_500_QUERY_EXECUTION_FAILED` on both paths, unchanged by this task.
