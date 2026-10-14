# Independent structured Ponytail validation — round 3

## Immutable subject and inputs

Wyrd base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`, candidate
`dc353ef5ecf39da13f3dcac5b9b5a31558b6a35e`; pinned DataFusion original
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`, candidate
`f059acf6c58b2c5fd4bc36b3358e8a56ac283476`; distributed fork unchanged
at `8fac2a978bdcf8e06457238ebfe9c7f35023c123`. Both candidate HEADs were
checked and unchanged during validation.

Inputs: subject, approved specification, absence of original task, prior
verdicts/validation and R1/R2 tasks including committed evidence, cumulative
Wyrd and fork diffs, both final implementation reports, standards, maintainer,
system, DataFusion and tenancy reports, and claim comparison. No follow-up
report exists: final discovery claims agree on the shared producer and were
independently tested here. Authority is AGENTS, agent-rules, specification
workflow, maintainer style, Bifrost/DataFusion/OLAP and error/testing ownership.
CodeGraph preceded navigation into Wyrd source. Candidate source stayed
immutable; only this assigned report is written.

## Proposal dispositions

| Discovery proposal | Disposition | Source-backed result |
|---|---|---|
| BEHAVIOR-1: FULL JOIN bound permits scalar cardinality bypass | CONFIRMED | `max_rows` returns product 1 for unmatched singleton FULL inputs, while their relation returns 2; shared scalar rewrite skips `single_value`. |
| INV-1: FULL JOIN and GROUPING SETS are sibling false-bound producers | CONFIRMED | Both branches understate row-expanding operators; grouping reproduction independently confirms that the incorrect bound survives optimization and bypasses validation. |
| DF-R3-1: cardinality closure incomplete through shared bound | CONFIRMED; correction consolidated | Same cause and original FIND-3 invariant, not a new finding per SQL shape or consumer. |
| Standards, maintainer, system and tenancy | No material proposals | Their scope/coverage is present; native aggregate/codec/state and unchanged authority/lifecycle owners are corroborated in source. |

Validated ledger has **one retained finding**, preserving
`FIND-bifrost-errors-3`. No new FIND ID is assigned to the grouping-set
manifestation or to the owner's invariant-oriented steering.

## Validated ledger

### FIND-bifrost-errors-3 — CONFIRMED, correction REVISED

- Discovery IDs: BEHAVIOR-1, INV-1, DF-R3-1.
- Classification: INCORRECT; incomplete closure of the existing finding.
- Obligation: REQ-006 scalar semantics and R2 criteria 1–3. An evaluated
  scalar dependency must refuse a second row, including equal and NULL rows,
  rather than silently behave as a set.
- Changed consumer: fork `datafusion/optimizer/src/scalar_subquery_to_join.rs:
  383–405` (particularly the `max_rows() <= 1` bypass).
- Invalid-state owner: fork `datafusion/expr/src/logical_plan/plan.rs:1446–1535`,
  specifically Aggregate at 1457–1468 and Join at 1486–1493.
- Sibling consumer: `datafusion/expr/src/logical_plan/invariants.rs:172–199`
  also treats a bound <=1 as proof for correlated scalar validation.
- Consequence: Analytical-configured Filter and Projection scalar queries
  still can return successful wrong results; the aggregate correctly refuses
  ordinary multi-row inputs but is absent from these plans.

### Source proof and reachable values

Read the complete `max_rows`, `Filter::is_scalar`, shared `build_join`, both
Filter/Projection rewrite branches and `single_value_udaf`, plus native scalar
extraction, accumulator update/merge/state/evaluate and distribution/codec
consumers. `max_rows` promises a guaranteed upper bound; `None` means unknown.
Projection, alias, window and repartition forward bounds, while ordinary
aggregate, limit/fetch, VALUES, empty relation, unions and join variants
produce or derive them. The only nonrecursive production callers found are
the scalar rewrite and correlated scalar invariant validation. This is one
existing proof owner, not an invitation to create a second scalar classifier.

For FULL JOIN, two disjoint one-row VALUES relations emit two unmatched
null-extended rows. The current branch shares the left/right product and
reports `Some(1)`. Projection and alias preserve it, so `build_join` skips the
asserting aggregate. For GROUPING SETS `((x), ())`, one input row produces one
grouped row and one global group, both with count 1. The Aggregate branch treats
this nonliteral grouping expression as nonexpanding and forwards input bound
`Some(1)`. The same shared shortcut again removes cardinality enforcement.
Duplicated output values are still two rows under scalar semantics.

These shapes can pass the Wyrd SELECT floor with an authorized real outer
table; they are not limited to table-free engine tests. For the existing
nonempty peer fixture, substitute its registered events table for `{events}`:

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

The real relation is discovered and authorized through the unchanged planner,
VALUES need no catalog authority, and the nested scalar goes through the same
Analytical-configured Filter/Projection producer. A Filter version compares
the outer id to either inner scalar. An ordinary relational FULL JOIN or
grouping query remains free to return several rows; it is their use as a
scalar that requires refusal. This does not expand supported query positions
or require execution of work legitimately eliminated by optimization.

### Executable falsification and provenance

Domain discovery's recorded FULL-join fixture first proves that registered
`single_value` refuses two VALUES rows, then fails its expected refusal for
the FULL-join scalar. Its trace contains the scalar LEFT JOIN over a FULL
JOIN without `single_value`; exactly one file ran, exit 1.

Independent validation used the one additional focused run authorized by the
orchestrator. Temporary fixture
`/tmp/bifrost-r3-cardinality/test_files/grouping_scalar.slt` sets the physical
scalar flag false, first checks the same direct aggregate refusal, then expects
the grouping-set scalar to refuse. Command:

```bash
env RUST_LOG=datafusion_optimizer=debug \
  /home/thorrester/Documents/GitHub/datafusion-scalar-snapshot/target/debug/deps/sqllogictests-5fda3bdc791d6a0c \
  --test-threads 1 grouping_scalar > grouping-output.log 2>&1
```

Working directory was `/tmp/bifrost-r3-cardinality`. Output at
`/tmp/bifrost-r3-cardinality/grouping-output.log` reports `1/1 files completed`,
exit 1, and `query is expected to fail, but actually succeed` for
`SELECT (SELECT COUNT(*) FROM (VALUES (1)) t(x) GROUP BY GROUPING SETS ((x), ())) AS v`.
The final traced optimized plan is Projection → LEFT JOIN → GROUPING SETS
Aggregate, with no `single_value`. The initial direct aggregate refusal passed.
This used an **existing binary**, modification time
`2026-10-14 18:26:12.082602728 -0400`, not a fresh rebuild or binary hash
attestation. Candidate checkout is f059acf6; source and successful aggregate
control corroborate relevance, with the provenance limitation retained.
No compilation, heavy lane, benchmark or synthetic load was performed.

### Decision-complete minimum correction

Fix DataFusion's existing `LogicalPlan::max_rows` proof producer. Select the
conservative approach: **report unknown for FULL JOIN and grouping-set
Aggregate expansion unless a mathematically sound upper bound is established**,
instead of forwarding/producting a false singleton bound. Unknown already
causes the existing shared scalar rewrite to insert `single_value`; no new
caller guard, query-shape allowlist, scalar evaluator or proof framework is
needed. Retain sound bounds for ordinary aggregates, VALUES, LIMIT/fetch and
nonexpanding operators; particularly keep genuinely global MAX and LIMIT 1
singletons eligible for the established fast path.

Treat every numeric bound returned by this same owner as a proof, including
empty inputs and arithmetic: join multiplication and union accumulation must
use representable, conservative arithmetic, returning unknown on overflow
rather than wrapping or panicking. This is the same bound invariant, not a
separate hardening project. A sound numeric FULL/grouping bound is an optional
private optimization, not needed to close the finding; conservative unknown
is the minimum safe correction. Do not add downstream FULL/grouping exceptions
to `build_join` or duplicate the proof in Wyrd. Legal multi-row relational
queries remain legal; unsafe **singleton proof state** must not be produced.

Keep the existing native `single_value`, normal AggregateUDF registry/codec
and partial/final row-count state. Empty yields typed NULL, singleton preserves
value/type/metadata, two rows refuse across batches and final merges. Final/
Single already require SinglePartition and distributed network coalescing
already requires Maximum(1); no new stage, scheduling, resource or concurrency
decision is necessary. Keep the distributed flag disabled, the two logical
passes, native equality build/filter readiness, first-poll Scribe snapshot,
static pruning and stable route ownership. Advance fork pin, lock and codec
fence together through their existing owner.

### Focused closure proof

1. Native direct upper-bound tests cover disjoint singleton FULL JOIN and
   singleton GROUPING SETS, their projection/alias propagation, empty sides,
   checked arithmetic overflow, and retained true-singleton controls. A bound
   must never be smaller than any reachable output; unknown is acceptable.
2. Flag-off native SQL Filter and Projection cases for both expanding inputs
   must refuse a second row, including grouping-set duplicate values; retain
   zero/singleton/NULL, cross-batch/global partial-final and codec proof.
3. One existing Analytical-configured public peer journey uses its real
   authorized outer table and a bounded-expansion scalar above to assert
   execution refusal. Extend the existing fixture/client, not a new framework.
4. Retain exact remote literal/footer equality proof, representative remote
   join, compact corpus and mapper proof. Use only narrow changed-owner lanes
   and normal concurrency; no benchmarks or whole-journey aggregate in this
   remediation. Integrated change review owns the final capability gate.

## Prior finding closure and other dispositions

- FIND-1 remains CLOSED for its diagnosed dropped available predicate and
  selective equality transfer. Current live code takes Pre static/Post dynamic
  predicates, retains the original expression, and snapshots at first poll.
  Native hash build/filter readiness precedes probe polling. Planned routes
  are dealt before narrowing; footer/fragment/literal assertions bound output
  transfer, not physical rows decoded or universal range optimization.
- FIND-2 remains CLOSED. The large generated workload/public oracle is absent;
  fixed small mixed-state fixtures and explicit expected answers cover common
  SQL including CTE joined to a second table. No ordinary testing DataFusion
  dependency was promoted. Human-directed coverage remains separate from FINDs.
- FIND-3 remains OPEN for the shared proof bypass above; ordinary unbounded
  scans, duplicate/null rows and global partial-state validation are improved
  and need no replacement machinery.
- FIND-4 is CLOSED. The mapper test and MCP query-error journey now document
  assertion panic conditions; assertions remain substantive and unchanged.
- Default SQL registration of native `single_value` is appropriate existing
  engine integration; no independent drift finding is validated.
- Current opaque 500 execution refusal is outside REQ-002's caller **planning**
  error promise and explicitly preserved by R2. No remapping finding is added.
- Earlier untraced 604-byte reservation observation remains undiagnosed; green
  rerun does not explain it. No changed assertion/retry/timeout conceals it,
  and no source-backed candidate leak finding is established here.

## Result and limits

Independent source validation retains one bounded implementation finding,
FIND-3. No material new product, public contract, protocol, lifecycle/resource
or persistent-data decision is required for its correction. Committed green
checks are credible for their asserted paths but do not cover the false
singleton proof. No candidate edits or heavy verification were performed.

**Validated result: FIX_REQUIRED**, with FIND-bifrost-errors-3 retained and
FIND-bifrost-errors-1, -2 and -4 closed.
