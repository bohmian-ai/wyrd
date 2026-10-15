# DataFusion / OLAP domain review — round 4

## Subject and coverage

Assigned immutable Wyrd subject: base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`,
candidate `7fe69545d0d8a2ab2a1e3033f52d5091e80d5b0e`; fork base
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`, candidate
`b2d37bc182b9d3e576650a414b364f3d9b5aefe3`; unchanged distributed fork
`8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

Authority: approved bifrost-errors spec REQ-006 / INV-001, R1–R3 tasks and
prior validation/verdicts, owner steering concerning sound singleton proof,
AGENTS/agent-rules, spec-driven development, maintainer style, reference router,
DataFusion, OLAP, Arrow analytical interop and reliability references. CodeGraph
was used before Wyrd source navigation. Review considers original-to-candidate
fork and Wyrd changes, not just the latest upper-bound delta.

| Boundary | Source / consumers inspected | Assessment |
|---|---|---|
| Native bound owner | complete `LogicalPlan::max_rows` match, Aggregate construction, `Filter::is_scalar`, unique-key functional dependencies, join dependency propagation, grouping-set dependency clearing | FULL/grouping/overflow repairs are sound; Filter sibling proof remains unsound |
| Bound consumers | shared `ScalarSubqueryToJoin::build_join`, both Filter/Projection callers, scalar extraction, logical scalar invariants | both rewrite callers trust the same remaining false singleton proof |
| Runtime scalar enforcement | complete `single_value` accumulator, return/state fields, update/merge/evaluate, registry, SQL/proto tests | empty -> typed NULL; one -> value; equal/NULL second rows refused; partial states preserve count across partitions |
| Snapshot/native execution | ScalarSubqueryExpr snapshot + metadata/reset tests; hash-join stream build readiness/report wait, probe polling | existing scalar and equality dynamic-filter mechanisms are appropriate owners |
| Wyrd remote boundary | LiveScribeExec pushdown, snapshot/resolved and route dealing; lazy LiveFragmentRead; two-pass session config; codec revision; analytical peer negative/literal/join journeys | retained R1 boundary avoids unresolved scalar transport, preserves leader authority and disjoint route ownership |
| Evidence | committed R3 exact selectors/results; actual bound test, flag-off SQL failures, partial/final proto refusal and peer journey assertions | proves identified FULL/grouping cases; does not cover dependent unique-key equality below |

## Prior closure

FIND-1 stays closed for available selective equality predicate shipping: native
hash join waits for build/report readiness before polling probe; leaf snapshots
once on first poll and partitions planned routes before further narrowing. The
two-pass logical configuration lets extracted equality become a native hash
key; MAX and LIMIT singleton controls still avoid added `single_value` work.
These are deterministic execution properties, not wall-clock benchmark claims.
FIND-2 stays closed: the generated general workload/oracle is absent and fixed
common-query fixtures remain. FIND-4 documentation repair is present.

FIND-3's FULL JOIN and GROUPING SETS manifestations are repaired: unknown bounds
insert native validation; grouping dependencies are cleared, FULL dependencies
are empty, arithmetic is checked, and empty outer sides retain sound bounds.
Ordinary inner/left/right products with positive sides are safe upper bounds;
semi/anti/mark and AsOf output no more than the retained side; Window,
Projection, Sort, Distinct, alias and repartition do not expand output; unnest,
recursive/extension and uncertain operators return unknown. The remaining
Filter shortcut is another producer of the same unsafe singleton proof.

## Proposed finding DF-R4-1 — existing FIND-bifrost-errors-3 remains open

- Classification: INCORRECT; incomplete closure of the existing singleton
  invariant, not a new finding for another query shape.
- Obligation: REQ-006 and R3 criteria 1–2: skip runtime cardinality validation
  only with a sound guarantee for the complete input.
- Invalid-state producer: fork `datafusion/expr/src/logical_plan/plan.rs:2909–2969`,
  `Filter::is_scalar`, consumed by `max_rows` at 1453–1455.
- Changed consuming shortcut: `datafusion/optimizer/src/scalar_subquery_to_join.rs:383–405`.
- Sibling consumers: both Filter and Projection rewrite branches share
  `build_join`; `logical_plan/invariants.rs:188` also trusts the same bound.
- Observable consequence: an evaluated scalar with two rows can still execute
  successfully as a set and multiply/filter outer rows, rather than refuse.

`Filter::is_scalar` identifies equality on all unique-key columns, but accepts
any opposing expression except another bare column. It never establishes that
this expression has one value for the entire filter invocation. Unique `x`
equal to `abs(n)` does not imply a singleton when `n` varies per input row.
Ordinary GROUP BY constructs a Single functional dependency for nonnullable
group key x covering every output column (`common/src/functional_dependencies.rs:
487–508`). Both groups below satisfy the predicate: x=1/count=1 and x=2/count=2.
The filter cannot push beneath the Aggregate because it uses aggregate output.

```sql
SELECT (SELECT x
        FROM (SELECT x, COUNT(*) n
              FROM (VALUES (1), (2), (2)) t(x)
              GROUP BY x) g
        WHERE x = abs(n)) AS picked
FROM (VALUES (0)) o(z);
```

An authorized real Bifrost outer table can replace the VALUES outer relation;
the SELECT-floor authorization sees that table and imposes no restriction
against these native inner VALUES/grouping expressions. This is the same public
reachability established by the existing FULL/grouping peer negatives.
The native uncorrelated rewrite sees propagated Some(1), omits `single_value`,
and replaces the scalar by a LEFT JOIN. Its retained join has no scalar
cardinality semantics capable of repairing the false proof.

### Focused reproduction

Orchestrator authorized one traced existing-binary SLT run outside candidates.
Fixture: `/tmp/bifrost-r4-filter-proof/test_files/filter_scalar.slt` sets the
physical scalar flag false, checks `single_value` directly refuses two VALUES
rows, then requires the scalar query above to refuse. Exact command, working
directory `/tmp/bifrost-r4-filter-proof`:

```bash
env RUST_LOG=datafusion_optimizer=debug \
  /home/thorrester/Documents/GitHub/datafusion-scalar-snapshot/target/debug/deps/sqllogictests-5fda3bdc791d6a0c \
  --test-threads 1 filter_scalar > /tmp/bifrost-r4-filter-proof/output.log 2>&1
```

Exit 1, exactly `1/1 files completed`; the direct aggregate control passes.
The scalar refusal expectation fails with `query is expected to fail, but
actually succeed`. Final traced plan contains Projection -> LEFT JOIN ->
Projection -> `Filter: t.x = abs(count(Int64(1)))` -> grouped Aggregate;
no `single_value` is present. Native optimizer completed two passes and still
retained the false-proof shape.

Binary provenance: existing executable mtime `2026-10-14 19:07:00.796568878 -0400`,
size 545421928 bytes; fork HEAD was b2d37bc18 and worktree clean. No fresh
build/hash attestation; source trace and successful registered aggregate control
corroborate applicability. No benchmark, synthetic load, broad lane or
compilation was performed.

### Minimum correction and proof

Correct `Filter::is_scalar` at the existing singleton-proof owner. Unique-key
equality proves a singleton only when the opposite value is constant for the
entire relevant invocation; dependence on the filtered relation or per-row
volatile evaluation must not establish that proof. Conservative false/unknown
is sufficient when constancy cannot be established. Reuse existing native
expression dependency/volatility facts rather than introducing SQL-shape
checks, a new proof framework or Wyrd guard. Preserve valid literal/static
unique-key equality and genuinely singleton MAX/LIMIT bounds.

Unknown already selects the shared native `single_value` aggregate for Filter
and Projection. Keep its state, global merge, codec, scalar null/empty rules,
native equality runtime filters and Wyrd snapshot/route ownership unchanged.
The proof owner must not claim singleton merely because a varying expression
has been wrapped in a cast/function/arithmetic node. This is one source
correction, not a downstream check for this particular `abs` query.

Focused closure: direct bound/Filter proof test with a unique key and
row-dependent opposite expression plus sound constant controls; native flag-off
Filter and Projection scalar rejection above, with native physical scalar
reference; a volatile opposite expression must not yield singleton proof;
extend the existing analytical-configured authorized peer negative journey.
Keep existing FULL/grouping/overflow, equal/NULL/global/codec and remote
literal/footer regressions green with narrow normal-concurrency checks.

## Immutability and result

During discovery, Wyrd HEAD changed from assigned `7fe69545d...` to
`17697dc8c2740213565e5f913cde1edfb0cb8ce5`, a merge of origin/main. This was
reported immediately to the orchestrator. Fork HEAD remains b2d37bc18 and
clean. Report-only changes are untracked under task-review-4; no candidate code
was edited here. The assigned cumulative review cannot certify the new merged
candidate under the skill's immutable-subject rule.

**Overall result: BLOCKED (candidate changed).** Independently of that blocker,
source and focused reproduction propose continued FIND-3 incompleteness above.
No other domain finding is proposed. Final ledger disposition belongs to fresh
independent validation, not discovery agreement.
