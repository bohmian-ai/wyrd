# Round-6 independent structured Ponytail validation

**PASS. Explicitly validated empty retained finding ledger.**

## Immutable subject and authority

Wyrd cumulative integration baseline
`945a1548c47897dc94583169a6ba04163a73a749` to candidate
`95b350d078226e811ac298230c233bf935975d42`; original task base
`526355fb8c58dae6d67d516f98dddcd5c02b0e79` remains recorded history.
Native DataFusion cumulative base `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`
to candidate `b8ec4b1d291cd8e371bb88fc76959d91e8503a1d` in
`/home/thorrester/Documents/GitHub/datafusion-scalar-snapshot`.
Latest native delta starts at `8c3f18e9806ae23e8840a710af775c058dfad66a`;
latest Wyrd remediation starts after `8a54d81c346db464b624e81ebe8958c63bb7e228`.
Distributed fork remains `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.
Both current HEADs matched these identities immediately before this report.

Approved revision-1 `../../spec.md`, owner directions and the prior
verdicts/ledgers/remediation packets supply acceptance authority; there was no
original implementation task. Applied the task-review skill, AGENTS and agent
rules, spec-driven and maintainer guidance, applicable Wyrd design/doctrine,
Bifrost, native DataFusion/OLAP and testing authority. The R5 task's appended
implementation evidence was assessed against actual source and assertions.

This is a fresh validator, separate from implementation and all seven current
discovery roles. Read the subject, comparison and all seven reports, then
independently inspected the cumulative executable/configuration changes,
native fork changes, owning functions, relevant callers and tests. CodeGraph
was consulted before Wyrd navigation. No executable tests, builds, benchmark,
load experiment, candidate edits, commit, push or deployment were performed.
Only this assigned report is written.

## Proposal dispositions

| Discovery report | Proposed material findings | Independent disposition |
|---|---|---|
| task-review-behavior.md | Empty; PASS | Empty union accepted after producer/consumer and public-assertion tracing below. |
| task-review-invariants.md | Empty; PASS | Empty union accepted after proof, partial/final state and runtime lifetime tracing. |
| standards-review.md | Empty; PASS | No retained rule violation found in the cumulative changed owners, manifests, test registration, docs or language surfaces. |
| maintainer-review.md | Empty; PASS | Existing native/Wyrd owners and native child walk are reused; no replacement workflow or unearned state owner established. |
| system-review.md | Empty; PASS | Query-local refusal/cleanup ownership is retained; no new process-wide failure or retry boundary established. |
| domain-review-datafusion.md | Empty; PASS | Fixedness, guaranteed bounds, both scalar consumers and native distributed merge independently checked. |
| domain-review-tenancy.md | Empty; PASS | Authorization precedes provider diagnostics; bound routes and authenticated fragment authority remain unchanged. |

No rejected proposal is retained as optional remediation. Agreement among the
reports is not the proof for this disposition. No conflicting claim or
untraced reachable common source requires another focused discovery role.

## Shared proof producer and all production consumers

Native `expr/src/logical_plan/plan.rs:2909–3010` has one Filter singleton
owner. Its complete predicate/key calculation retains conjunction semantics,
nonnullable Single dependencies covering the output and complete composite
keys. Equality contributes a key only if its opposite value, in either order,
passes `is_fixed_value`. The old rule accepting anything except a bare Column
is absent. A row-dependent expression cannot become fixed merely by adding a
cast, function, operator or CASE parent.

`is_fixed_value` accepts literal/parameter/OuterReferenceColumn leaves. An
outer reference is fixed for this inner invocation, not constant across all
outer rows. Its ordinary pure family includes operators, casts, scalar
functions, CASE, BETWEEN, IN-list, LIKE/SIMILAR TO and null/truth tests.
ScalarFunction volatility is checked before descending; Volatile returns
false. The composition arm uses native `Expr::apply_children`, not a second
child inventory. Inspection of `expr/src/tree_node.rs:45–115` establishes that
CASE visits its optional base, every WHEN/THEN pair and optional ELSE; BETWEEN,
IN-list and pattern operators visit every expression operand. A false child
returns Stop, and only Continue grants proof. Unsafe conditions and unused
branches therefore cannot hide a varying input. Unknown variants, inner
columns, variables, nested plans, aggregates, windows and binding/unnest forms
remain unproven. Stable/Immutable rely on the existing native function
volatility contract; no new purity contract is invented.

The private fixedness helper is called only by `Filter::is_scalar`, which
feeds `LogicalPlan::max_rows` (`plan.rs:1450–1457`). A source search across
native DataFusion confirms the other production consumers of max_rows are
`logical_plan/invariants.rs:188` and
`optimizer/src/scalar_subquery_to_join.rs:385`, in addition to its recursive
operator propagation. Both consequences were checked:

- Correlated nonaggregated scalar admission requires a proven bound <=1;
  it has no general uncorrelated runtime fallback. Ordinary pure outer CASE
  and sibling compositions now retain this proof without relaxing the analyzer.
- Shared scalar-to-join `build_join` is called by both Filter and Projection.
  For uncorrelated unknown/>1 inputs it adds the existing native `single_value`
  aggregate before the scalar relation is exposed to the join. Missing aggregate
  registration leaves the rewrite unapplied rather than joining a set.

Read the complete producer and build_join bodies. No consumer acquires a
query-shape exception or duplicates fixedness. Functional-dependency producers
remain consistent: FULL JOIN drops dependencies; grouping sets do not acquire
ordinary aggregate uniqueness; a nonnullable ordinary grouped key does.
The rest of max_rows retains exact/nonexpanding bounds or unknown; populated
FULL JOIN and grouping-set aggregation return unknown, and join products/UNION
sums use checked arithmetic. Genuine global aggregate and LIMIT singletons
retain the established fast path.

## Cumulative acceptance and prior closure

| Prior finding / obligation | Independently checked source and consequence | Disposition |
|---|---|---|
| FIND-bifrost-errors-1: lost selective live predicate | Native ScalarSubqueryExpr snapshot produces a metadata-preserving Literal or pending error. ScalarSubqueryExec populates state before main-input execution. Wyrd live.rs retains Pre static/Post dynamic predicates and snapshots at first poll; native hash join waits for build/report readiness before probe polling. Planned routes are partitioned before further narrowing, preventing reassignment/duplication. Two session optimizer passes retain native equality-key extraction. Remote proof asserts exact two fragments/two footer rows and both literal bounds. | CLOSED for diagnosed selective equality transfer, without a universal inequality/scan-cost claim. |
| FIND-bifrost-errors-2: broad workload machinery | Cumulative implementation has no generated workload/public query oracle or dependency promotion. The compact two-table mixed published/live Rust corpus specifies explicit results for projection/group/HAVING, inner/left join, scalar zero/one/NULL, correlation, membership/EXISTS, joined CTE and window. Existing public fixtures and clients own setup. | CLOSED; human-directed common-query coverage remains distinct from reviewer findings. |
| FIND-bifrost-errors-3: wrong multi-row scalar results | single_value counts rows, including equal and NULL rows, using value plus 0/1 count. Empty contributes typed NULL; second input/merged row errors before scalar output. Native partial/final state, default registration and codec remain the sole mechanism. Distributed network-coalesce enforces one consumer task below native coalescing/final merge. FULL/grouping/overflow and input-dependent Filter proof repairs remain present. | CLOSED for diagnosed scalar enforcement and false-bound bypasses. |
| FIND-bifrost-errors-4: missing Panics documentation | Current mapper unit and MCP planning-error journey document assertion panic conditions; assertions remain substantive. | CLOSED. |
| FIND-bifrost-errors-5: valid correlated pure expressions refused | One native positive proof now covers the approved pure family via complete native child traversal. Direct PK correlation remains; SQL adds PK CASE, grouped-key CASE and compound-family positives with exact unmatched NULL, plus dependent/volatile negatives. Existing peer join journey adds grouped required-id CASE through one real public client, expecting 107→108 and 199→NULL. | CLOSED. |
| REQ-002 actionable logical errors with internal redaction | Oracle plan_logical uses scoped typed SQL/Plan/Schema/NotImplemented leaf classification through Context/Diagnostic/Shared/Collection, resource precedence and strip_backtrace. General execution remains opaque; physical errors use fixed repair detail. Existing derived catalog/transport is reused. First-class/MCP assertions cover suggestions, coercion, ambiguity and negative disclosure. | PASS. |
| REQ-006 live scalar/IN/EXISTS and managed run_id | Existing native planning owns SQL semantics; live leaf and correlation fixture use existing write/client owners. Rust/Python/TS and MCP journeys compare explicit expected ids against equivalent joins over live rows. | PASS within recorded cumulative execution evidence. |
| INV-001 / applicable negative flow | Table extraction includes subqueries/CTEs; planner read_and_materialize authorizes resolved identities before object/provider materialization. The session registers admitted providers. Route pruning only removes pre-bound routes; Scribe verifies tenant, binding, fences, digests and assignment authority before follower IO. Scoped joined-table negatives assert refusal without protected-column suggestions. | PASS. |
| Pin/codec identity and scope | Eleven root patches, git lock sources and ORACLE_DATAFUSION_REVISION agree on b8ec4b1d2; distributed/Arrow universe stays unchanged. New plan/footer observations are test-support-only and extend existing Scribe/tally owners. No new public node/protocol/evaluator/framework appears. | PASS. |

Native `single_value` accumulator, registration and scalar-to-join behavior
remain appropriate installed mechanisms. The partial count state has only
0/1-producing native writers before an error; no arbitrary external count
writer is added. `NetworkCoalesceExec` construction in the distributed fork
(`inject_network_boundaries.rs:364–395`) requires Maximum(1), so partition-local
counts remain globally validated. The native roundtrip refusal test protects
function identity/state after encode/decode.

Query errors remain query-scoped. Live snapshots precede opening selected
fragments; lazy stream/drop and existing request bindings retain deadline,
cancellation and admission ownership. Test instrumentation records after
authenticated follower startup and does not create production plan retention.
The correction introduces no durable state, retry, shared-server crash action
or alternate resource lifecycle. No stronger new availability guarantee is
needed to accept this bounded change.

## Proof assessment and diagnosed fixture correction

Inspected `filter_is_scalar_only_for_fixed_key_values`: 23 composition
constructors place a probe in the relevant child positions, with fixed
outer/literal positives and inner-column/volatile negatives, in both equality
orders. Existing direct/arithmetic/cast/function/parameter, composite-key,
nullable-key and grouped-bound controls remain. These are deterministic proof
tests; the random() SQL negative asserts analysis refusal, not random output.

The current subquery.slt contains the three formerly refused valid correlated
queries, exact expected matching/unmatched rows, correlated row-dependent and
volatile CASE refusals, uncorrelated row-dependent CASE refusal, count/abs
Filter/Projection refusal, physical reference/direct single_value control,
FULL JOIN/grouping cases and singleton/NULL/empty controls. The peer negative
journey asserts the execution-failure code for each of six queries; the peer
positive journey asserts the exact grouped CASE rows. The transfer journey
retains its literal/footer/fragment assertions. These protect both consumers
of the shared proof rather than assuming runtime fallback covers correlation.

R5 evidence records the exact proof unit and 1/1 subquery SQL file, expr265,
optimizer886, single_value4 and proto **library17**, plus Wyrd focused counts
2/4/3/1 and fmt/lints/codegen/dependency/tenant checks. Those selectors/assertions
are consistent with current source. Library17 is not a fresh execution of the
historical external proto265 suite; no such claim is accepted. R5 explicitly
records no new all-520 SQL run. Older broad results support unchanged portions
of the cumulative change but are not relabeled as execution on the latest pin.

The duplicate API-key claim was independently checked: the query helper calls
provision_public_api_key("live-subquery") each time; current join/CASE journey
provisions once and uses one Bifrost client for both requests, as the refusal
journey does. This matches the documented cards_identity_unique failure and
fixes its setup producer. Neither query/result assertion, refusal code,
timeout nor retry was weakened. The evidence records a stack trace, not a
captured successful-run trace count; the latter is not claimed here.

## Final ledger and limits

**Retained material findings: none.** No new FIND ID or remediation task is
required. Prior FIND-bifrost-errors-1 through -5 are source-validated closed.
The native producer now prevents the diagnosed unsafe singleton proofs while
preserving ordinary fixed correlated proofs needed by its sibling consumer.
It does not claim all future optimizer defects are impossible.

Recorded checks are assessed evidence, not fresh validator executions. No
benchmark, synthetic host load, measured latency claim or full integrated
capability gate is supplied by this review; integrated change review remains
a separate completion step. Structural performance evidence retains static
pruning, readiness, bounded transfer and native singleton bypasses. Historical
planning-cost comments are not current measurements. Runtime scalar refusal's
opaque 500 is explicitly preserved and outside the approved planning-detail
contract. The earlier untraced memory-reservation observation remains
unexplained; no changed assertion or source-backed new leak is established.

**Validated result: PASS.**

Skipped: no executable verification reruns, builds or benchmarks.
Risk: acceptance is bounded by the recorded focused evidence and inspected
native contracts; it is not universal query/performance certification.
