# Round 5 invariant review

Overall result: **FAIL**. The diagnosed row-dependent singleton bypass is repaired at its native producer. The new positive proof nevertheless rejects an ordinary fixed CASE expression, and one sibling consumer turns the missing proof into a planning refusal for a previously valid correlated scalar lookup. This is an observable compatibility regression rather than the reported extra runtime check.

## Immutable subject and authority

Wyrd `945a1548c47897dc94583169a6ba04163a73a749` → `8a54d81c346db464b624e81ebe8958c63bb7e228`; original history base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`. DataFusion `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` → `8c3f18e9806ae23e8840a710af775c058dfad66a`; latest fork delta starts at `b2d37bc182b9d3e576650a414b364f3d9b5aefe3`. Distributed fork is unchanged. HEAD was reconfirmed at the stated Wyrd candidate during this review.

Authority: approved `spec.md` revision 1; no original task existed. Prior verdicts and R1–R4 remediation packets establish the cumulative obligations and hypotheses. Applicable repository authority includes AGENTS, agent-rules, Wyrd design/doctrine, Bifrost design, reference router, spec-driven-development, maintainer-style, DataFusion/OLAP/error/testing references, and TESTING. Owner instructions require native fixes, common-query coverage, preservation of valid queries, no benchmarks and no consumer query-shape hacks. This is a fresh independent invariant discovery pass; no other current discovery report was read.

CodeGraph was consulted before expanding the Wyrd map. The cumulative Wyrd runtime/test/config diff and native fork changes were inspected, rather than treating the latest pin change as the task. No source edits, builds, benchmarks, synthetic load or test reruns were performed. Green execution below is recorded implementation evidence, not my rerun.

## Obligation matrix

| Requirement, acceptance criterion, constraint or non-goal | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 caller logical-planning faults carry actionable 4xx details | `oracle/mod.rs:3735–3801`: typed wrapper traversal and logical mapper retain the selected SQL/Plan/Schema/NotImplemented cause; physical mapper redacts detail | Mapper unit covers bare/context/diagnostic/shared/collection forms and resource precedence; shipped surface negative tests retained | PASS |
| REQ-002 internal failures stay 5xx and unauthorized schema remains private | Logical and physical mapping are separate; query cut/provider authorization precedes registration; generic execution mapping remains opaque | Existing unauthorized-table assertions across Rust/Python/TS/MCP and recorded mapper/check evidence | PASS |
| REQ-006 live scalar/IN/EXISTS and correlated run_id cases | Native logical/physical execution and authoritative leader predicates retained; query fixture stamps two shared run_ids and one different run_id through existing enqueue correlation | Rust/Python/TS/MCP equivalent-row journeys retained; mixed live/published corpus | PASS for required covered forms |
| INV-001 tenant isolation | No new identity, authorization, SQL pool, tenant-filter or fragment trust path; private fragment resolution/fingerprint checks precede record/execution | Recorded tenant-isolation check; cross-surface unauthorized tests retained | PASS |
| AC-001 every shipped first-class/agent surface has user journeys | Rust query_bifrost/query_coverage, Python test_query_bifrost, TS query-bifrost, MCP bifrost query assertions drive existing public paths | Prior focused language/MCP evidence and current focused Rust evidence are recorded | PASS within task verification |
| AC-002 applicable negative flows | Unauthorized-table errors remain detail-free; caller query diagnostics are typed; unrelated Jev/traceparent obligations belong to other split changes | Negative journey assertions and mapper control | PASS |
| R1 native ready/pending/NULL/metadata/reset snapshot semantics | Fork ScalarSubqueryExpr::snapshot reads current result slot and creates native Literal with metadata; pending slot returns error, reusable expression keeps runtime binding | Native snapshot tests and Wyrd scalar snapshot unit; recorded green | PASS |
| R1 planning prunes only known conjuncts; execute snapshots after readiness | live.rs handle_child_pushdown_result retains Pre/static and Post/dynamic expressions; LiveFragmentRead::into_stream resolves on first poll | Focused static-plus-pending unit and peer scalar journey | PASS |
| R1 each planned route has one partition owner before further pruning | live.rs:298 partitions planning-time route order with skip/step_by before selected_by; each snapshot only drops routes in its share | Unit unions both partition shares across two scalar values | PASS |
| R1 selective analytical equality uses native hash filter and literal transport | Shared OracleSessionShape sets two optimizer passes; native hash build readiness precedes probe polling; live snapshots native dynamic filter | Remote scalar journey requires exactly two fragments/two footer rows and both static/resolved literal values in fragment | PASS |
| R1 public behavior retained and no distributed flag reversal | analytical planning_session retains with_distributed_planner; no new evaluator/planner/remote expression protocol | Existing peer journey and codec tests; recorded green | PASS |
| R1/FIND-2 remove broad generated harness and dependency promotion | Compact fixed two-table corpus; ordinary wyrd-testing dependency graph unchanged; no public workload framework | Cumulative diff/manifests; recorded check:deps | PASS |
| Owner common-query coverage including joined CTE | query_coverage has explicit expected rows for filters/aggregates/inner and outer joins/scalar/correlation/IN/NOT IN/EXISTS/NOT EXISTS/CTE/window; both published and live rows | Exact corpus test recorded green; CTE a is joined from second relation b | PASS |
| R1 representative remote live join | Existing PeerCluster, public client and two live table fixture, no replacement harness | Remote join exact rows journey recorded green | PASS |
| R2 global scalar row-count semantics, including equal and NULL rows | Native SingleValueAccumulator holds 0/1 row count, rejects second update/partial row; native aggregate final merge combines counts | Aggregate unit controls, SQL cases, native proto roundtrip, analytical published/live duplicate journey | PASS |
| R2 zero/one/NULL and proven singleton behavior | Typed NULL accumulator initialization; max_rows controls wrapping only uncertain uncorrelated inputs; MAX and LIMIT remain native | Aggregate, SQL, corpus and codec controls recorded green | PASS |
| R2 shared filter/projection rewrite and native encoding | Both rewrite callers use build_join and same registered single_value; ordinary aggregate registry/state/proto boundary reused | Both projection/filter negative peer queries; proto roundtrip recorded green | PASS |
| R3 FULL JOIN/GROUPING SETS/overflow bounds | max_rows returns unknown for nonempty FULL JOIN and grouping sets, uses checked product/sum | Exact native row-bound test, SQL negative cases and extended existing peer refusal journey | PASS |
| R4 input-dependent/volatile/opaque values cannot create singleton proof | plan.rs:2942 guard calls positive is_fixed_value; inner columns and volatile functions fail recursively; opaque forms fail closed | Native exact unit covers both orders, arithmetic/cast/function negatives, composite completeness, nullable key, grouped count input bound | PASS |
| R4 grouped count/abs scalar Filter and Projection refuse | False singleton proof is no longer produced; max_rows fallback is input bound; build_join installs single_value | New flag-off SQL and six-query analytical negative journey recorded red then green; reference physical/direct aggregate controls | PASS |
| R4 direct and arithmetic/cast/function outer-reference lookups retain proof | Positive fixed leaves include OuterReferenceColumn and compositions | Exact native unit and retained unique correlated SQL query | PASS for enumerated controls |
| R4 preserve established valid correlated queries generally | New catch-all rejects pure fixed CASE, and correlated analyzer requires proof rather than adding runtime validation | Current controls omit fixed CASE; source trace below establishes analyzer refusal | FAIL, INV-R5-1 |
| FIND-4 mandatory Panics documentation restored | Changed mapper and MCP test docs now name panic conditions | Source documentation and recorded fmt/lint gates | PASS |
| AC-003 pin/lock/codec and owning checks | Cargo patch entries/lock source and ORACLE_DATAFUSION_REVISION agree on 8c3f18e98 | Recorded focused selectors (2/4/3/1 tests), fork expr/optimizer/proto/single_value and SQL suite; fmt/lints/codegen/deps/tenant checks | PASS within recorded evidence |
| Non-goals: no new protocol/resource/lifecycle owner or broad generator; no benchmarks | Existing native expression/aggregate/rewrite/codec and Wyrd live leaf owners extended; Scribe footer/plan observation is cfg(test-support) | Cumulative source diff, task evidence and this source-only review | PASS |

## Producers, consumers and reuse

The singleton authority remains `LogicalPlan::max_rows`. Its Filter branch delegates to Filter::is_scalar, whose positive proof intersects fixed equalities with nonnullable complete unique-key dependencies. GROUP BY and native table constraints produce those dependencies. The latest repair correctly prevents the group `(x,n)=(1,1),(2,2)` from gaining a false bound through `x=abs(n)`.

The important sibling consumers differ:

1. `ScalarSubqueryToJoin::build_join` (`optimizer/src/scalar_subquery_to_join.rs:383–405`) wraps an **uncorrelated** uncertain input in registered native single_value. Both Filter and Projection rewrites call it. An unknown proof here adds a runtime check.
2. `check_subquery_expr` (`expr/src/logical_plan/invariants.rs:166–196`) accepts a **correlated nonaggregated** scalar input only if max_rows proves at most one; an unknown proof here returns a planning error. It does not reach the uncorrelated aggregate fallback.

For cumulative live routing, the leaf keeps the runtime-bound predicate while a per-execution clone receives a native immutable snapshot. Planned route ownership is assigned before snapshot pruning, so later dynamic restrictions cannot duplicate routes across partitions. The leader retains correctness filters. Fragment construction, authenticated peer dispatch, footer validation, query grants and cancellation remain their existing owners; test-support observations reuse the Scribe record hooks and native tally rather than creating a second execution or transport owner.

For cardinality, native partial counts and final coalescing preserve a global invariant; neither codec nor Wyrd performs a second row-count policy. Current conservative FULL JOIN/grouping-set bounds and checked arithmetic remain sound. The fixed-expression helper is small pure logic beside its real producer, not an evaluator or generalized constancy framework. It is the correct correction boundary, but its conservative fallback has an unpreserved sibling consequence below.

## Proposed material finding

### INV-R5-1 — REGRESSION: fixed CASE can turn a valid unique correlated scalar lookup into a planning error

**Violated obligation:** R4's preserved valid correlated lookups and owner-directed no-regression requirement; the approved task adds native subquery correctness without removing existing query forms. R4 explicitly identified the correlated invariant checker as a sibling consumer that must be preserved. This is a compatibility consequence of the same changed proof producer, not a proposal to add a general correlated evaluator.

**Location:** DataFusion `datafusion/expr/src/logical_plan/plan.rs:2970–2985`, new `is_fixed_value` catch-all, consumed by `logical_plan/invariants.rs:188–193`.

**Concrete query using the existing native test fixture:**

```sql
SELECT t1_id,
       (SELECT t3_int
        FROM t3
        WHERE t3.t3_id = CASE WHEN t1.t1_id > 0
                             THEN t1.t1_id ELSE -1 END) AS t3_int
FROM t1;
```

`subquery.slt:439` already pins the supported direct lookup using t3's nonnullable primary key. Its setup has t1 ids 11/22/33/44 and t3 ids 11/22/44/55, so this CASE lookup is semantically the same fixed lookup and should return `(11,3),(22,1),(33,NULL),(44,3)`.

**Producer-to-consumer source evidence:**

- Every CASE condition/result here uses only the outer invocation's id and literals; no inner-row column or volatile operation can change the key between inner rows.
- Before the latest repair, Filter::is_scalar accepted this opposing non-Column expression. With the complete nonnullable t3 primary key, it established Some(1).
- The new helper accepts only Literal/Placeholder/OuterReferenceColumn, selected unary/binary expressions and scalar calls. Expr::Case falls through to false even when every component is fixed. Filter::max_rows therefore falls back to the unbounded table scan.
- `TypeCoercionRewriter::f_up`, `analyzer/type_coercion.rs:834–836`, coerces and preserves Expr::Case. Analyzer's normal rules are ResolveGroupingFunction and TypeCoercion (`analyzer/mod.rs:87–93`); it runs Executable invariants at lines 159–162, before logical optimization can simplify the CASE.
- `check_subquery_expr` sees outer references and a nonaggregated inner plan, then refuses because max_rows is unknown at invariants.rs:188–193, with `Correlated scalar subquery must be aggregated to return at most one row`.
- `build_join`'s runtime single_value fallback is expressly restricted to `outer_ref_columns.is_empty()`. Consequently the implementation packet's claim that excluded fixed forms merely get a runtime check and unchanged results is not true for this sibling.

**Observable consequence:** a previously supported, mathematically single-row correlated lookup now fails planning. A fully fixed literal CASE on the key can have the same effect in a correlated query whose separate predicate supplies its outer reference. The original wrong-result bypass is closed; this is a regression introduced by overly narrow proof construction.

**Smallest correction:** retain the positive proof at the existing Filter owner, preserving the rejection of inner-column dependence, volatile functions and opaque nested runtime forms. Recognize ordinary native pure CASE composition when all selector/condition/result/else expressions are invocation-fixed, and inspect the same ordinary pure expression family for this identical sibling consequence rather than fixing queries in Wyrd or weakening the correlated analyzer. Unknown opaque/plan-bearing forms must remain unproven. No new proof type, optimizer invocation, evaluator, runtime parameter store or protocol is needed.

**Focused closure proof:** extend the existing fixed-key proof test with positive CASE over outer/literal values and negative CASE containing an inner column or volatile expression (in a condition or result), in both equality orders. Extend the existing native subquery SQL file with the concrete primary-key query and exact expected rows, retaining its direct lookup control, prior arithmetic/cast/function controls and grouped count/abs refusals. Check the ordinary pure forms that the candidate explicitly lists as newly excluded for the same correlated planning consequence. The existing focused Wyrd scalar/literal/codec journeys and pin consistency remain the appropriate write-set proof when advancing the fork; no broad benchmark or new harness is justified.

**Verification limit:** this reviewer did not execute the proposed query, inspect another current reviewer's results or run an old checkout. The finding is a source-derived regression proposal for independent validation, not a claimed reproduced failure. Its old/new proof difference, analyzer order and failure sink are explicit above.

## Prior-finding closure and limits

FIND-1 stays closed for its diagnosed selective equality/literal transfer scope, including the current source-backed snapshot timing and disjoint route ownership. FIND-2 stays closed: small fixed corpus and existing native tooling replace the generated workload. FIND-4 stays closed: required panic documentation is present. The prior FIND-3 count/abs unsafe-proof diagnosis is closed by the current predicate guard; INV-R5-1 is a compatibility consequence of that same proof owner for validator reconciliation and stable-ID assignment.

Recorded broad fork SQL success is useful but does not cover query forms absent from its fixtures. No recorded check was rerun here. No unrelated performance redesign, inequality-join enhancement, public 4xx remapping of execution-time cardinality errors or new framework is requested. The earlier unexplained query-reservation observation remains a prior verification limitation, with no newly established source defect in this pass.
