# DataFusion / OLAP domain review — round 5

## Subject, authority and scope

Reviewed Wyrd `945a1548c47897dc94583169a6ba04163a73a749` →
`8a54d81c346db464b624e81ebe8958c63bb7e228`, and native DataFusion
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` →
`8c3f18e9806ae23e8840a710af775c058dfad66a`; the latest native delta starts
at `b2d37bc182b9d3e576650a414b364f3d9b5aefe3`. Distributed fork remains
`8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

Authority: approved revision-1 spec, owner instructions preserving correct
common/correlated queries and requiring native source-owner correction, AGENTS,
agent-rules, spec-driven-development, maintainer-style, reference router,
DataFusion/OLAP references, and the applicable Bifrost design planning/execution
contract. Prior verdicts and R4 remediation were read as hypotheses and authority,
not proof of current closure. CodeGraph was used for Wyrd navigation.

This is a separate fresh native-query/OLAP discovery review. No other current
report or intended verdict was supplied. Candidate source is unchanged.

## Source coverage and assessment

| Boundary | Evidence inspected | Assessment |
|---|---|---|
| Filter singleton proof | `expr/src/logical_plan/plan.rs:2910–2988`, new full helper and unit test | Positive whitelist correctly excludes row-varying columns hidden by arithmetic/casts/functions; stable/immutable functions require every argument fixed. Native `expr-common/src/signature.rs:56–85` explicitly guarantees stable per-query output for fixed input. Opposite equality order, complete composite key and nullable exclusion remain intact. |
| Shared consumers | `LogicalPlan::max_rows:1450–1545`; `logical_plan/invariants.rs:157–211,397–405`; both `ScalarSubqueryToJoin` callers and complete `build_join` | Unknown uncorrelated bounds install runtime single_value; unknown correlated bounds can instead fail planning. This distinction exposes the regression below. |
| Other guaranteed bounds | FULL/LEFT/RIGHT/INNER and semi/anti/mark joins, GROUPING SETS, literal/global grouping, union, limit/fetch, window, projection, alias, distinct and opaque fallbacks | FULL nonempty and grouping sets remain conservative; multiplication and sums checked; genuine singleton controls retained. No new bypass located in these branches. |
| Native runtime scalar validation | Complete `functions-aggregate/src/single_value.rs`; registry/rewrite/physical codec roundtrip additions | Zero → typed NULL, one including NULL → value, multiple equal/NULL rows → error; native partial states carry value/count, final merge counts partitions together. No new node/protocol or Wyrd evaluator. |
| Runtime predicate transfer | Native ScalarSubqueryExpr snapshot; Wyrd LiveScribeExec resolved/pushdown/execute/read; OracleSessionShape; native hash_join stream states | Static pruning retained; planned route share assigned before narrowing; execution-local first-poll snapshot retains reusable runtime expression; hash build/report precedes probe polling. Two logical passes reuse native equijoin extraction and avoid the previously observed NLJ equality shape. |
| Focused product proof | Existing peer live_subquery journey, compact query corpus and native SQL/proto controls; pin/lock/codec delta | Literal/footer test asserts two fragments and two transferred rows plus static/resolved literal; six refusal queries include count/abs filter and projection. These are meaningful assertions, not a benchmark or a physical-storage-row count. |

The proof correction's positive form is appropriate and small. It does not
need a second engine, custom query classifier, expression evaluator, new proof
type, dependency or execution protocol. Its closed default is preferable to
negatively checking only columns/volatility, because opaque expression forms
cannot accidentally acquire proof. The remaining issue is compatibility for
native pure forms whose uncertainty has a different consequence in a sibling
consumer.

## Proposed material finding

### DF-R5-1 — REGRESSION — fixed outer CASE lookup is newly rejected

**Obligation:** REQ-006 correlated support and the owner/R4 requirement to preserve
valid established queries while fixing the shared singleton-proof producer.

**Changed location:** native `datafusion/expr/src/logical_plan/plan.rs:2973–2987`,
`is_fixed_value`, wildcard rejection of `Expr::Case`; sibling consumer
`datafusion/expr/src/logical_plan/invariants.rs:188–194`.

**Producer-to-consumer trace:** a unique key compared to a CASE made entirely
from outer references and literals has exactly one comparison value during an
inner invocation. The previous proof accepted that expression. The new helper
returns false for every Case regardless of its children. Consequently Filter
falls back to an unknown/multirow input bound. The uncorrelated rewrite can use
single_value, but the existing correlated scalar invariant checker instead
requires a guaranteed bound <=1 for the nonaggregated lookup and refuses with
“Correlated scalar subquery must be aggregated to return at most one row”.
`strip_inner_query` strips Projection/SubqueryAlias, not Filter. This happens
after Analyzer, before the logical optimization that might otherwise simplify
expressions; an outer-dependent CASE cannot simply constant-fold anyway.

A direct native reproduction can extend the existing `subquery.slt:439` PK
control:

```sql
SELECT t1_id,
       (SELECT t3_int FROM t3
        WHERE t3.t3_id = CASE WHEN t1.t1_id > 0
                             THEN t1.t1_id ELSE 0 END)
FROM t1;
```

Every outer id in that fixture is positive, so expected rows are the same
`11 3`, `22 1`, `33 NULL`, `44 3`. Every inner comparison is a unique-key lookup;
no multirow scalar can occur. Nevertheless the new proof cannot establish it.
The same source mechanism is reachable without physical PK constraints via a
nonnullable grouped key under a derived alias:

```sql
SELECT o.id,
       (SELECT g.id
        FROM (SELECT id, COUNT(*) n FROM events GROUP BY id) g
        WHERE g.id = CASE WHEN o.id > 0 THEN o.id ELSE 0 END)
FROM events o;
```

Here Aggregate builds a unique Dependency::Single
(`common/src/functional_dependencies.rs:483–508`), SubqueryAlias preserves it,
and the inner Filter's immediate input is SubqueryAlias rather than Aggregate,
so the correlated checker uses max_rows. Replacing events with the authorized
Bifrost relation reaches the ordinary SELECT path; no Wyrd PK feature is needed.

**Observable consequence:** valid correlated scalar queries can newly fail
planning. The implementation evidence's general assertion that excluded CASE
forms merely gain runtime cardinality checks is true for the uncorrelated
consumer but false for this sibling correlated consumer. This is not a request
to accept unsafe row-dependent CASE or to weaken the correlated validator.

**Smallest correction:** retain the native Filter proof owner and closed positive
proof; recognize ordinary native pure expression forms such as CASE only when
all relevant operands/conditions/branches are invocation-fixed under the same
existing contract. Preserve rejection of inner columns, volatile functions,
opaque subplans, variables, lambdas and unsupported uncertain forms. Review the
other conservatively rejected ordinary pure forms against this same correlated
consumer together, rather than discovering one caller/operator per round. Do
not add a Wyrd query-shape guard, unconditional scalar acceptance, alternative
validator, evaluator or generic constancy framework. A direct existing native
constant/invariance mechanism may be reused only if it proves outer-invocation
invariance soundly; generic no-column/nonvolatile traversal is insufficient.

**Focused closure proof:** existing correlated PK SQL control plus CASE of outer
references returns identical rows before/after; equivalent grouped-derived
lookup establishes the Wyrd-relevant shape. Include inner-dependent/volatile
CASE negative proof controls, retain count/abs filter/projection refusal and
the existing arithmetic/cast/function fixed controls. Inspect all max_rows
consumers, not only scalar-to-join. No broad query generator or benchmark needed.

This is proposed for independent validation; source establishes the changed
branch and consumer consequence. I did not execute a reproduction. Orchestrator
has serialized any execution through another discovery role to avoid duplicate
work/shared Cargo activity.

## Prior findings and verification limits

- FIND-1 remains closed for diagnosed equality predicate loss: native snapshot,
  hash readiness, first-poll snapshot and exact route dealing are retained.
- FIND-2 remains closed: compact explicit mixed-state query corpus replaces the
  removed generated workload and normal dependency promotion.
- FIND-3's row-varying count/abs bypass is corrected, and accumulator/global
  merge/FULL/grouping/overflow repairs remain sound in inspected source;
  DF-R5-1 is a regression at that same proof boundary, not a new Wyrd workaround.
- FIND-4 is outside this domain's documentation gate; the previously corrected
  test Panics sections are present in the cumulative source inspected.

The R4 implementation table records focused unit (1), SQL file (1), expr (265),
optimizer (886), proto (265), single_value (4), all SQL (520), focused Wyrd
selectors (2/4/3/1), formatting/lints/codegen/dependency/tenancy checks. I checked
named source assertions/registrations and recorded commands; these are recorded
implementation results, not my reruns. Compound outer-reference controls are
proof-owner unit controls, while the existing end-to-end correlated SQL control
covers only a direct outer reference. Thus their green result does not exercise
CASE. No builds, tests, benchmarks, synthetic load or broad lanes were run here.
No latency result is inferred from the historical “tens of microseconds” comment.

## Overall result

**FAIL** — DF-R5-1 requires independent validation. The demonstrated row-dependent
false-singleton cause is fixed, but the same producer now rejects a sound
invocation-fixed expression whose correlated consumer has no runtime fallback.
