# Round 5 — independent behavior review

**FAIL.** The row-dependent-value bypass is repaired, but the new conservative
classifier also rejects valid correlated scalar lookups instead of adding a
runtime check. One material proposal, BEH-R5-1, is submitted for independent
validation.

## Subject and coverage

Reviewed Wyrd `945a1548c47897dc94583169a6ba04163a73a749` →
`8a54d81c346db464b624e81ebe8958c63bb7e228`, original base
`526355fb8c58dae6d67d516f98dddcd5c02b0e79`; DataFusion
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` →
`8c3f18e9806ae23e8840a710af775c058dfad66a`, with latest delta from
`b2d37bc182b9d3e576650a414b364f3d9b5aefe3`. Distributed revision is unchanged.
HEAD was rechecked at the candidate. No candidate source was modified.

Authority: approved revision-1 spec, owner instructions, AGENTS and agent
rules; spec-driven-development and maintainer references; error, testing,
DataFusion and OLAP references; applicable Bifrost query/terminal authority,
Wyrd client/error architecture and doctrine; prior verdicts and remediation
packets, including R4. There was no original implementation task. Earlier
findings were hypotheses requiring current source evidence.

CodeGraph preceded Wyrd code searches. Expanded navigation covered logical
error mapping, SQL table floor and provider registration, live filter capture,
first-poll resolution/fragment serialization, resources configuration,
test-only fragment evidence, first-class SDK/MCP assertions and corpus. Native
tracing covered Filter/max_rows, functional-dependency production, correlated
subquery analyzer, scalar-to-join callers and aggregate, snapshot and codec
controls. Current other discovery reports were not read.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 caller SQL planning failures retain actionable planner detail as 4xx | `oracle/mod.rs:3746–3819`: typed caller cause traversal, logical mapper exposes stripped caller cause, physical mapper retains safe fixed detail; resource precedence retained | Rust/Python/TS `query_bifrost` assert suggestions for single/join/subquery, type mismatch and ambiguity; MCP `query.rs:217–410` also checks unknown function and structured problem fields; recorded green lanes | PASS |
| REQ-002 internal failures remain opaque 5xx | General mapper remains distinct; no arbitrary external/error-string classifier; mapper unit exercises internal and execution collection leaves | `datafusion_query_rejections_are_safe_and_actionable`; recorded exact run | PASS |
| REQ-002 and INV-001 unauthorized metadata is not exposed | `parse_select_tables` enumerates referenced Bifrost relations; authorized context/cut registers only admitted providers before logical planning; no global catalog added | Rust/Python/TS unauthorized-table journeys assert absence of typo and private verdict column; existing isolation owners retained | PASS |
| REQ-006 scalar/IN/EXISTS and run_id correlation on live tables | Native plan/snapshot changes and LiveScribeExec extend existing DataFusion pushdown; no Wyrd subquery parser/executor | Four explicit live cases and equivalent joins in Rust/Python/TS and MCP; exact expected ids, including managed run_id; recorded green runs | PASS for named supported cases |
| REQ-006 and owner preservation of valid established queries | `Filter::is_scalar` now uses positive fixed-value classifier, but `Expr::Case` is always false; correlated analyzer consumes max_rows as admission condition | Fresh traced focused reproduction below: direct correlated controls pass, CASE-over-outer versions fail before execution | FAIL |
| R1 selective predicate transfer and no duplicate/missing routes | `live.rs:298–317` deals each partition's planned routes before runtime narrowing; `:441–476` Pre/Post predicate capture; `:668–676` resolves at first poll; fragment wraps retained predicate in FilterExec | Remote scalar journey asserts picked id, exactly two fragments, two footer rows, static and resolved literals; scalar execution unit checks per-execution values | PASS |
| Native planning and performance boundary | Shared `OracleSessionShape::session_config` uses two optimizer passes; native hash-build/filter readiness retained; distributed physical scalar flag remains disabled | Existing native SQL and remote literal/footer proof; no benchmark claim is established or required here | PASS within stated proof |
| R2 scalar cardinality for genuinely unproven uncorrelated inputs | Shared `scalar_subquery_to_join.rs:383–405` wraps unknown/>1 inputs with native single_value for both filter and projection; accumulator counts rows including NULL/equal values and merges native partial states | SQL controls, four accumulator tests, codec round-trip, published/live peer refusal | PASS |
| R3 FULL JOIN/GROUPING SETS/overflow bounds | `LogicalPlan::max_rows:1450–1524` returns unknown for relevant FULL/grouping cases; checked product/sum | Current bound tests, SQL filter/projection refusals, peer negative cases; recorded suites | PASS |
| R4 row-dependent arithmetic/cast/function cannot create singleton proof | `is_fixed_value` recursively rejects input Column, volatile scalar calls and opaque leaves; either operand order guarded; conjunction/composite/nullability rules unchanged | New native exact test, count/abs SQL filter/projection controls, Analytical peer queries; recorded red/green | PASS |
| R4 literals, placeholders and direct/compound outer references preserve valid proof | Literal/Placeholder/OuterReferenceColumn, cast, binary and nonvolatile scalar-function arms retained | Native test controls and existing unique correlated SQL case | PASS for tested arms; broader valid CASE regression above |
| Human-directed common-query coverage includes joined CTE | Compact 16-case fixed two-table corpus with explicit results; inner/left joins, scalar empty/NULL/singleton, correlated sum, IN/NOT IN, EXISTS/NOT EXISTS, aggregate/HAVING, CTE join, rank | `query_coverage::common_queries_answer_over_published_and_live_rows`; owning mise journey selector includes module | PASS |
| AC-001 shipped SDK and MCP/HTTP journey coverage | Real SDK→server fixtures retained; tests write acknowledged live rows and compare exact results; MCP drives official client | Recorded surface journey runs plus focused current fork peer/corpus runs; no new runtime binding behavior added | PASS within recorded evidence |
| AC-002 scoped negative flows | Unauthorized metadata cases retained. Jev/upstream/traceparent obligations are inherited from parent split and not changed by this Bifrost-only task | No claim of new proof for unrelated inherited behaviors | PASS for scoped obligation |
| AC-003 focused checks and rustdoc | New mapper/test docs include Panics; pins/lock/codec revision agree; report records fmt/lints/codegen/deps/tenant checks | Recorded implementation exits/counts assessed, not rerun; see limits | PASS within evidence |
| Non-goals and smallest native correction | No Wyrd SQL-shape guard, replacement optimizer/codec/protocol, benchmark, query generator or new constancy framework | Cumulative source and latest delta inspected; compact corpus replaces prior broad harness | PASS |

## BEH-R5-1 — valid correlated CASE lookup is refused before runtime fallback

- Classification: **REGRESSION**.
- Obligation: preserve established valid queries while repairing the shared
  singleton proof, under REQ-006 and R4's preserved behavior/owner direction.
- Changed location: fork `datafusion/expr/src/logical_plan/plan.rs:2970–2985`,
  especially the default-false arm of `is_fixed_value`.
- Sibling consumer: `datafusion/expr/src/logical_plan/invariants.rs:172–195`.
  Unlike the uncorrelated scalar rewrite, it requires a proven bound to admit
  a nonaggregated correlated scalar subquery and produces a planning error
  when that proof disappears.

`CASE WHEN outer.id > 0 THEN outer.id ELSE -outer.id END` is fixed for an
inner invocation: every input-dependent leaf is an outer reference, and its
operators are pure. The new classifier nevertheless declines it solely
because its root is Expr::Case. The old classifier accepted that non-Column
value. `max_rows` now falls back to the multirow/unknown input bound and the
correlated analyzer refuses the query. No single_value is inserted: the
shared scalar rewrite explicitly limits that mechanism to uncorrelated
subqueries (`scalar_subquery_to_join.rs:384`). The risk statement that omitted
forms merely receive runtime validation therefore does not hold for this
reachable sibling consumer.

This is not dependent on exposing PRIMARY KEY DDL through Bifrost. GROUP BY
on an ordinary required nonnullable id produces a unique dependency, retained
through projection/SubqueryAlias. Both of these public SQL shapes have the
same singleton property:

```sql
SELECT o.id,
       (SELECT g.id
        FROM (SELECT i.id, COUNT(*) n
              FROM vala.bifrost.events i GROUP BY i.id) g
        WHERE g.id = CASE WHEN o.id > 0 THEN o.id ELSE -o.id END) AS picked
FROM vala.bifrost.events o;
```

Replacing the CASE with `o.id` succeeds in the focused native fixture;
the CASE version fails analyzer validation. Bifrost's SELECT/table floor
accepts this query shape, and the normal authorized provider path supplies
the required id. The failure occurs during logical analysis, before the
interactive/distributed choice; it is not a worker protocol restriction.

Producer/sibling trace: native GROUP BY functional dependency → Filter fixed
value classifier → max_rows → correlated subquery analyzer refusal; sibling
uncorrelated Filter/Projection rewrite trusts the same bound but can insert
single_value. The error maps to an actionable planning refusal in Wyrd,
whereas the valid query should return its rows. Native decorrelation already
accepts ordinary Filter correlation and transfers its expression to a join;
no new execution mechanism or consumer bypass is justified.

Required correction: preserve a sound positive invocation-invariance proof
for ordinary pure native CASE expressions built entirely from fixed parts
at this existing producer, and review sibling pure scalar forms handled by
the same classifier against the correlated admission consumer. Keep input
columns, volatility and uncertain nested/runtime scopes unproven. Do not
weaken the correlated analyzer or replace the sound classifier with the old
non-Column rule. Prove direct and grouped CASE-over-outer lookups stay correct,
and CASE containing an inner column/volatile branch cannot grant singleton
proof; retain count/abs rejection. This is one source correction, not a
series of Wyrd guards. Independent validation must select final scope and ID.

## Focused execution and provenance

Orchestrator authorized a traced existing-binary reproduction, with no build
or candidate edits. Fixture:
`/tmp/bifrost-r5-correlated-proof/test_files/correlated_case.slt`.
Current fork HEAD was
`8c3f18e9806ae23e8840a710af775c058dfad66a`, worktree clean.
Binary:
`/home/thorrester/Documents/GitHub/datafusion-scalar-snapshot/target/debug/deps/sqllogictests-5fda3bdc791d6a0c`,
size 545424088 bytes, mtime `2026-10-14 20:14:15.736732463 -0400`.
It was not rebuilt or hash-attested; source evidence establishes the same
current refusal path independently.

From `/tmp/bifrost-r5-correlated-proof`:

```bash
env RUST_LOG=datafusion_optimizer=debug \
  /home/thorrester/Documents/GitHub/datafusion-scalar-snapshot/target/debug/deps/sqllogictests-5fda3bdc791d6a0c \
  --test-threads 1 correlated_case > output.log 2>&1
```

Final run selected exactly **1/1 files**, exit **1**, and reported exactly
two query errors: the primary-key CASE query and grouped-key CASE query.
The direct outer-reference primary-key and grouped-key controls passed.
The trace records each CASE containing outer_ref leaves after type coercion,
then `Invalid (non-executable) plan after Analyzer` / `Correlated scalar
subquery must be aggregated to return at most one row`.
Final fixture uses `t1_id INT NOT NULL` to match an ordinary required Bifrost
id. An intermediate grouped fixture accidentally left this field nullable;
both grouped controls then refused under the existing nullable-key rule.
That setup error was corrected; its log remains `nullable-output.log` and
is not counted as proof of a regression. The initial primary-key-only log
is preserved as `first-output.log`. No old checkout/baseline run was used.

## Prior finding assessment and limits

FIND-1's diagnosed predicate loss is closed by native snapshot plus Wyrd
first-poll/native dynamic-filter integration and disjoint route ownership.
FIND-2's broad generator is absent; fixed compact cases remain. FIND-3's
demonstrated unsafe row-dependent bound is now corrected, and prior
FULL/grouping/global-count/codec repairs remain intact; BEH-R5-1 concerns the
same proof owner's newly conservative effect on its correlated sibling.
FIND-4's Panics documentation is present in the changed mapper and MCP test.

Recorded green suites credibly cover their assertions, not this omitted
correlated CASE case. No benchmarks, synthetic load, broad lanes or builds
were run in this review. No live Wyrd server was started for the new finding;
public reachability is source-traced and its native logical behavior was
executed with a nonnullable grouped key. The earlier unexplained memory
reservation observation is not assigned a finding without a cause. Runtime
scalar cardinality's opaque 500 remains outside REQ-002's planning-error
contract; no status redesign is proposed.
