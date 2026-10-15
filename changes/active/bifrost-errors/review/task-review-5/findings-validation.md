# Independent structured Ponytail validation — round 5

## Subject and method

Wyrd candidate `8a54d81c346db464b624e81ebe8958c63bb7e228`, integration baseline
`945a1548c47897dc94583169a6ba04163a73a749`, original task base
`526355fb8c58dae6d67d516f98dddcd5c02b0e79`; previous reviewed candidate
`17697dc8c2740213565e5f913cde1edfb0cb8ce5`. Native DataFusion original
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`, previous
`b2d37bc182b9d3e576650a414b364f3d9b5aefe3`, current
`8c3f18e9806ae23e8840a710af775c058dfad66a`. Distributed fork remains
`8fac2a978bdcf8e06457238ebfe9c7f35023c123`. Both current HEADs were checked.

Inputs: subject, comparison, all seven current discovery reports, approved
revision-1 spec, prior verdicts/ledgers/remediations and their recorded execution
evidence. No original task existed. The spec and owner directions govern;
R4 explicitly preserves valid compound correlated lookups. Applied repository
AGENTS/agent rules, spec-driven development, maintainer guidance, applicable
native DataFusion/OLAP/error/testing/tenant authority, and the explicitly
requested 5why method. CodeGraph preceded focused Wyrd code navigation.
Cumulative runtime/test/config diffs and current native source were inspected;
reviewer agreement and implementation summaries were not treated as proof.
Only this report is written. No tests, builds, benchmarks or candidate edits
were performed by this validator.

## Proposal dispositions

| Discovery proposal | Disposition | Independent source result |
|---|---|---|
| BEH-R5-1 | CONFIRMED | Invocation-fixed CASE is rejected at the changed proof producer; the correlated analyzer refuses the valid lookup. |
| INV-R5-1 | CONFIRMED | The same bound has two materially different consumers: uncorrelated runtime checking and correlated admission. The latter has no claimed fallback. |
| DF-R5-1 | CONFIRMED; correction REVISED | Preserve the existing positive proof, but cover the ordinary pure native scalar-expression family together, with all operands fixed. Do not repair only CASE. |
| Standards, maintainer, system, tenancy | No material proposal | Their scoped PASS results do not prove compatibility of every consumer. Statements about conservative runtime fallback apply only where a fallback actually exists. |

There is one retained finding. Prior **FIND-bifrost-errors-3** is closed for its
diagnosed unsafe singleton/cardinality bypass. The current defect has a different
observable consequence introduced by that remediation: refusal of a valid query,
not successful multi-row scalar output. It receives the next unused ID,
**FIND-bifrost-errors-5**, rather than silently changing the meaning of FIND-3.
The two belong to the same shared proof boundary and need no separate corrections
per expression or caller. Owner-directed common-query coverage is not assigned
a retrospective finding ID.

## Final deduplicated ledger

### FIND-bifrost-errors-5 — CONFIRMED, correction REVISED — REGRESSION

**Obligation:** REQ-006, owner preservation of established valid queries, and
R4's explicit preserved behavior for compound correlated singleton lookups.

**Changed producer:** native `datafusion/expr/src/logical_plan/plan.rs:2963–2987`,
`is_fixed_value`, called by `Filter::is_scalar:2909–2959` in either equality
operand order. In particular its default-false arm rejects `Expr::Case`.

**Affected consumers:** `LogicalPlan::max_rows:1450–1543` propagates the Filter
proof through Projection/SubqueryAlias and other row-preserving plans.
`logical_plan/invariants.rs:157–211`, especially :188–194, uses that bound to
admit a nonaggregated correlated scalar. `optimizer/src/scalar_subquery_to_join.rs`
Filter/Projection callers (:121–132, :191–202) share `build_join:363–455`, whose
runtime `single_value` insertion (:383–405) is limited to uncorrelated scalars.

**Observable consequence:** a previously valid correlated unique-key lookup now
fails planning with “Correlated scalar subquery must be aggregated to return at
most one row”. It does not merely incur the runtime row-count check described in
the implementation risk statement. In Wyrd that native refusal becomes the
existing caller SQL planning 4xx; the correct result should have been rows.

### Independent producer-to-consumer trace

The new classifier correctly rejects input Columns even under arithmetic,
casts or immutable functions. It correctly treats an OuterReferenceColumn as
fixed within one inner invocation and checks scalar-function volatility against
`expr-common/src/signature.rs:56–85`: Stable is fixed for given inputs within a
query, Immutable across queries, Volatile may vary per evaluation.

However, an ordinary CASE over only outer references/literals is equally fixed
within that invocation. It has one comparison value for every inner input row.
Its exclusion loses a valid singleton proof without making the query itself
multirow. The correlated checker strips Projection/SubqueryAlias (:397–405),
then requires a known maximum <=1 for this nonaggregated lookup. It does not add
`single_value` when the proof is missing. The analysis refusal happens before
logical constant simplification; an outer-dependent CASE cannot simply fold to
one global literal anyway.

Concrete native case, using the existing unique-key lookup shape:

```sql
SELECT t1_id,
       (SELECT t3_int FROM t3
        WHERE t3.t3_id = CASE WHEN t1.t1_id > 0
                             THEN t1.t1_id ELSE -t1.t1_id END) AS picked
FROM t1;
```

With outer ids 11,22,33,44 and inner primary-key rows (11,3),(22,1),(44,3),(55,3),
the correct result is `(11,3),(22,1),(33,NULL),(44,3)`. The old non-Column arm
accepted CASE; the current helper rejects it. This source delta establishes the
regression without checking out or running an older baseline. The native
correlation pull-up already extracts correlated Filter expressions into the join
(`optimizer/src/decorrelate.rs:178–198`); no new execution support is required.

Reachability does not require exposing PRIMARY KEY DDL through Bifrost. Native
`common/src/functional_dependencies.rs:483–508` constructs a Single dependency
for an ordinary nonnullable GROUP BY key. Alias/projection preserve it. The
following ordinary authorized Bifrost shape therefore reaches the same proof:

```sql
SELECT o.id,
       (SELECT g.id
        FROM (SELECT i.id, COUNT(*) n
              FROM vala.bifrost.events i GROUP BY i.id) g
        WHERE g.id = CASE WHEN o.id > 0 THEN o.id ELSE -o.id END) AS picked
FROM vala.bifrost.events o;
```

Use a registered required, nonnullable `id`, as current fixtures already do.
The derived alias means the correlated checker sees Filter over SubqueryAlias,
not its special immediate-Aggregate case. Wyrd `parse_select_tables` at
`oracle/mod.rs:3233–3282` accepts one SELECT with authorized Bifrost references;
it imposes no CASE or derived-grouping exclusion. Existing catalog/cut
permission checks remain authoritative. Analysis occurs before Interactive vs
Analytical physical selection, so this is not a remote expression-codec limit.

### Executed evidence assessed, not rerun

Read `/tmp/bifrost-r5-correlated-proof/test_files/correlated_case.slt` and
`output.log`. The fixture has direct PK and grouped-derived outer-reference
controls, then equivalent CASE versions with the same expected results. Exactly
1/1 SQL files completed; only the two CASE queries failed, with the analyzer
error above. The log preserves their outer_ref expressions after type coercion.
The direct controls passed. The final grouped fixture uses NOT NULL; the earlier
nullable-fixture mistake is retained separately and is not evidence of this
regression.

Discovery's command, from `/tmp/bifrost-r5-correlated-proof`:

```bash
env RUST_LOG=datafusion_optimizer=debug \
  /home/thorrester/Documents/GitHub/datafusion-scalar-snapshot/target/debug/deps/sqllogictests-5fda3bdc791d6a0c \
  --test-threads 1 correlated_case > output.log 2>&1
```

Exit 1, exactly two query errors. Existing binary size 545424088 bytes, mtime
`2026-10-14 20:14:15.736732463 -0400`; fork HEAD is the stated clean candidate.
The binary was neither rebuilt nor hash-attested during review. Current source
independently proves the same refusal. No live Wyrd server execution of the new
case is claimed; product reachability is source-traced and the nonnullable
grouped native shape was executed.

### Decision-complete smallest correction

Extend the **existing native Filter singleton-proof producer** to recognize
ordinary pure scalar-expression composition only when **every value-bearing
child is invocation-fixed**. Keep the positive closed proof and fixed leaves;
retain its current accepted arithmetic, casts and nonvolatile scalar functions.
Cover this concrete native family together:

- CASE: optional base, every WHEN condition/value, every THEN and optional ELSE;
- BETWEEN: tested value and both bounds;
- IN-list: tested value and every list item;
- LIKE and SIMILAR TO: value and pattern, with their static flags/escape metadata;
- null/truth predicates: IsNull/IsNotNull, IsTrue/IsFalse/IsUnknown and their
  IsNotTrue/IsNotFalse/IsNotUnknown counterparts.

These are the ordinary pure scalar forms in `Expr` (:326–443) omitted by the new
classifier. Their semantics preserve a fixed value when their children are
fixed. Native `Expr::apply_children` (`expr/src/tree_node.rs:45–105`) already
enumerates those children, including all CASE branches, and the native constant
evaluator recognizes these forms (`optimizer/src/simplify_expressions/
expr_simplifier.rs:675–696`). Reuse native traversal where it avoids manual
child omission; keep eligibility positive. Do not add a second evaluator or
invoke constant folding recursively from `max_rows`. ConstEvaluator itself
requires different binding/evaluation context and is not a drop-in proof of
outer-invocation invariance.

All input-column dependence, volatile calls, scalar variables, subplans,
aggregate/window/grouping/unnest forms and higher-order/lambda binding forms
remain unproven. Unknown/new variants must not gain proof by default. A pure
parent cannot hide an unsafe descendant, even in a condition or unused branch.
This correction preserves the deliberately conservative opaque forms approved
in R4; it does not promise arbitrary new subquery or lambda support.

Do not relax the correlated analyzer, restore the old non-Column rule, change
nullable/composite-key rules, or add Wyrd query-shape guards. Do not install
runtime correlated validation or a new protocol/node to compensate for missing
proof: the native producer can establish the already-supported ordinary case.
Keep current bounds, `single_value`, shared rewrite, codec and execution owners.
The pin, lock and codec revision must continue to move together if the fork
revision changes. This is a bounded implementation correction, not a new product
or architecture decision.

The Ponytail ladder rules out deletion (it restores false bounds), existing
consumer fallback (absent for this correlated path), and a new dependency or
framework (native Expr/traversal/volatility contracts suffice). One producer
repair addresses all consumers and pure siblings; no per-query guards survive.

### Focused closure proof

1. Extend existing native proof-owner tests with positive outer/literal
   compositions for every pure family above and negative input-dependent and
   volatile descendants, in either equality order. For CASE include searched
   and simple forms, conditions/base, THEN/ELSE and optional ELSE. Test proof
   deterministically; do not rely on random output. Keep composite/nullable and
   direct/arithmetic/cast/function controls.
2. Extend the existing subquery SQL file: retain the direct unique PK control;
   require CASE PK and nonnullable grouped-derived correlated lookups to return
   exact rows, including an unmatched outer row. Include representative pure
   siblings so one fixed CASE special case cannot stand in for the family.
3. Extend an existing real public Bifrost journey/corpus with the grouped-key
   correlated CASE query and exact expected rows, using a required id. This
   proves the authorized product path without a new harness or SQL oracle.
4. Retain count/abs filter/projection refusal, physical-scalar reference and
   direct single_value controls, FULL JOIN/grouping/overflow proof, native
   aggregate merge/codec, and remote literal/footer/route assertions. Run only
   focused selectors and the narrow write-set checks needed for the resulting
   fork/pin/test changes; record nonzero exact commands/counts. No broad journey
   aggregate or benchmark is required by this remediation.

## Prior closure from current source

| Prior finding | Disposition and source evidence |
|---|---|
| FIND-bifrost-errors-1 | CLOSED for diagnosed selective equality predicate loss. Native ScalarSubqueryExpr snapshot returns typed Literal/metadata; Wyrd live.rs retains Pre/Post predicates, snapshots at first poll after build readiness, and deals planned routes before narrowing. Two native optimizer passes produce the equality hash filter; existing remote test asserts two fragments/two footer rows and both literals. No exhaustive inequality optimization claim. |
| FIND-bifrost-errors-2 | CLOSED. General generated workload/public qualification machinery and dependency promotion are absent. The fixed compact mixed published/live two-table corpus has explicit rows, joins/subqueries/CTE/window controls, with DataFusion dev-only in testing. |
| FIND-bifrost-errors-3 | CLOSED for the diagnosed scalar semantic failures. Native single_value counts equal/NULL rows and globally merged partial counts; shared uncorrelated rewrite applies it for unknown/>1 bounds. FULL JOIN/grouping-set/overflow bounds remain soundly conservative; current positive Filter guard rejects hidden input dependence and volatility. Current six-query analytical refusal proof includes count/abs filter/projection. The compatibility regression above is distinct from manufacturing a false singleton. |
| FIND-bifrost-errors-4 | CLOSED. Required Panics docs remain on the materially changed mapper and MCP negative test; earlier trailing verdict whitespace correction remains. |

Recorded green fork suites and Wyrd selectors/checks cover their actual
assertions. They omit the failing correlated CASE path; green results do not
prove its absence. Error mapping remains typed and scoped, authorization
precedes provider planning, tenant-bound route/fragment authority is unchanged,
and no new process failure/retry/cleanup owner was introduced. No independent
standards, tenancy, resilience or maintainability defect was established.

## Evidence-backed causal consolidation (5why)

**Problem:** making singleton proof safe has newly refused a valid correlated
CASE lookup. **Why 1:** the correlated analyzer requires a guaranteed <=1 bound
and rejects unknown (:188–194). **Why 2:** the Filter no longer proves that bound
because its new fixed-value classifier always rejects CASE (:2986). **Why 3:**
the positive classifier includes some fixed scalar compositions but omits other
ordinary pure ones, even when all leaves are outer/literal and fixed for the
invocation. That is the earliest supported repair boundary; no deeper
organizational explanation is evidenced or needed.

The earlier rounds were not all the same independent bug: the caller-error
mapping and discarded remote predicate were separate boundaries; broad workload
machinery was a scope issue. Within scalar correctness, however, the failures
were connected: unchecked scalar-to-join semantics needed single_value; then
false FULL JOIN/grouping-set bounds and row-dependent Filter bounds bypassed
that check. Those latter failures shared the guaranteed-row-bound authority.
The latest change repairs its unsafe Filter proof but exposes the opposite side
of the same contract: a valid correlated query needs a sound proof that remains
expressible. Current evidence supports one compatibility correction at that
owner, not a Wyrd redesign or more consumer guards.

**Why checks missed this is separate:** the new unit's fixed controls explicitly
list literal/placeholder/direct outer/arithmetic/cast/function, and the SQL
correlated control uses direct outer equality. None exercises CASE through the
correlated analyzer. Uncorrelated refusal journeys prove safe fallback for a
different consumer. Earlier bound tests targeted the then-demonstrated operators;
they did not prove every producer/consumer interaction. This is source-visible
coverage, not a claim about implementer intent or an invented incident cause.
The complete pure family plus both consumer consequences is the bounded
countermeasure; no generated exhaustive-query framework is justified.

## Result and limits

**FIX_REQUIRED — FIND-bifrost-errors-5.** All current proposals consolidate into
one source correction. No unresolved discovery conflict needs another role.
No new product, public API, protocol, resource or persistence decision is needed.

This validator ran no executable checks or benchmarks. Existing focused native
reproduction and source establish the regression with the provenance limits
above. Recorded broad/focused implementation checks are assessed, not rerun;
the later integrated capability gate remains change-review work. The historical
untraced memory-reservation observation remains unexplained, with no new causal
finding or weakened assertion inferred from it.
