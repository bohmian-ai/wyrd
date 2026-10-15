# Round-6 independent behavior review

**PASS. No material proposed findings.**

## Subject, method and limits

Reviewed Wyrd `945a1548c47897dc94583169a6ba04163a73a749` →
`95b350d078226e811ac298230c233bf935975d42`, retaining original task base
`526355fb8c58dae6d67d516f98dddcd5c02b0e79` and the prior review/remediation
packets as authority and hypotheses. Native cumulative range:
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` →
`b8ec4b1d291cd8e371bb88fc76959d91e8503a1d` in the specified fork worktree.
Both HEADs matched those identities at final source inspection. No original
implementation task existed. Approved revision-1 spec, owner directions and
R1–R5 remediation tasks define acceptance.

Read skill, repository agent rules and applicable design/doctrine, Bifrost,
testing, spec-driven and maintainer authority. Consulted CodeGraph before Wyrd
code navigation; expanded its starting map through owning modules, callers,
actual cumulative diffs and test assertions. Did not read another round-6
reviewer's report. No source modification, build, test execution, benchmark,
load generation or network factual claim is part of this report. Recorded
execution is implementation evidence assessed against source, not an
independent rerun. This report does not claim universal optimizer correctness
or measured query latency.

## Obligation matrix

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 caller planning errors preserve actionable detail | `oracle/mod.rs` logical lowering uses `map_sql_planning_error`; typed SQL/Plan/Schema/NotImplemented causes traverse Context/Diagnostic/Shared/Collection; resource exhaustion takes precedence | `datafusion_query_rejections_are_safe_and_actionable`; actual Rust/Python/TS and MCP single/join/subquery suggestion, coercion and ambiguity assertions | PASS |
| REQ-002 internal and physical diagnostics stay safe | Logical internal/execution leaves use general opaque mapper; physical failures use fixed repair detail; execution mapper remains opaque and preserves catalog/resource identity | Mapper negative assertions; first-class and MCP negative journeys retained | PASS |
| REQ-002 unauthorized names stay hidden | SQL table extraction/authorization and admitted provider registration precede planner suggestions; no authorization widening in cumulative diff | Rust/Python/TS unauthorized verification-table typo cases assert 403 and no typo/corrected column leakage; MCP refusal assertions retained | PASS |
| REQ-006 live scalar, IN, EXISTS, correlated run_id return equivalent rows | Native planner remains owner; Wyrd leaf retains execution-bound predicates; fixture stamps shared/other managed run_id through existing correlation write path | Rust/Python/TS and MCP live-only cases assert explicit expected ids and equivalent joins, including run_id; fixture leaves all rows unsealed | PASS |
| INV-001 tenant isolation | Existing AuthorizedQueryContext, pinned cuts, execution bindings and signed tenant-bound fragments remain authority; helper adds no tenant-selected source or wire capability | Existing isolation and unauthorized-table negatives; recorded check:tenant-isolation green | PASS |
| AC-001 shipped first-class and agent-facing journeys | Actual Rust SDK `query_bifrost`, Python/TS integration tests and real MCP journey retained; shared HTTP transport remains common client owner | Earlier cumulative surface runs recorded; current narrow peer/corpus runs recorded; no current all-surface rerun claimed | PASS within recorded evidence |
| AC-002 applicable Bifrost negative flow | Authorization failure occurs before unsafe dependency detail can be emitted | Explicit unauthorized-column leakage assertions above | PASS |
| AC-002 unrelated carried-over Jev/traceparent clauses | No Jev or traceparent behavior is changed by this split task; no new obligation to modify adjacent features | N/A for this bounded spec split | PASS |
| AC-003 format/lints/codegen/owning checks | Fork identities advance together in Cargo.toml, lock and codec revision; focused changes remain in native/Wyrd owners | R5 evidence records fork expr265/optimizer886/single_value4/proto17, fmt/clippy; Wyrd 2/4/3/1 exact selections plus fmt/lints/codegen/deps/tenant and diff checks | PASS within recorded evidence |
| R1 selective remote scalar transfer | Two logical passes enable native hash key/build readiness; live Pre/Post filter capture snapshots at first poll; resolved leaf partitions planned routes before narrowing and encodes static predicate | Exact unit reset/route/encoded-literal assertions; remote journey requires exactly 2 fragments, 2 returned footer rows, both static and resolved literals | PASS |
| Owner common-query coverage, joined CTE | Compact fixed two-table corpus uses explicit expected rows, mixed published/live state, existing SDK; no independent evaluator/generator | Cases include projection/filter/order/limit, grouping/HAVING, inner/left joins, scalar zero/one/NULL, correlated aggregate, IN/NOT IN/EXISTS/NOT EXISTS, joined CTE, rank | PASS |
| R2 scalar cardinality in both shared rewrite consumers | `ScalarSubqueryToJoin::build_join` wraps possibly-many uncorrelated input with registered native single_value; Filter and Projection call same owner; missing function leaves rewrite unapplied | Native zero/one/equal/NULL/partial-merge controls, native codec roundtrip; remote filter/projection published+live duplicate refuses | PASS |
| R3 sound FULL/grouping/overflow bound | Native max_rows returns unknown for populated FULL JOIN and grouping sets, checked join/union arithmetic; no Wyrd query-shape patch | Bound tests and native scalar cases; existing remote refusal journey retains FULL and grouping cases | PASS |
| R4 input-dependent/volatile key expressions cannot prove scalar | Existing Filter::is_scalar admits key equality only when opposing value satisfies is_fixed_value; closed default rejects columns, volatile, nested/opaque forms | Native proof-owner tests exercise both orders; count/abs filter/projection refusal remains in native SQL and remote journey | PASS |
| R5 complete ordinary pure fixed-value composition | Native helper now admits CASE/BETWEEN/IN-list/LIKE/SIMILAR/null/truth variants only through all-child fixed proof, preserving existing fixed leaves/operators/casts/nonvolatile functions | 23 expression shapes × fixed/row-dependent/volatile probes × both orders; correlated PK/grouped/sibling SQL exact rows plus negatives; remote grouped CASE positive including unmatched row | PASS |
| Scope and performance constraints | No new evaluator, protocol, proof framework, caller fallback or correlated analyzer relaxation; native aggregation only when bound cannot guarantee <=1; test-support plan retention is feature-gated | Structural source and transfer-count proof; no benchmark claim | PASS |

## Attempted falsification and common owner

The newest helper in native `expr/src/logical_plan/plan.rs:2963–3010` is a
positive proof, not a general expression traversal that accidentally accepts
opaque leaves. `ScalarFunction` volatility is checked before descent. Its
accepted composition arm invokes native `Expr::apply_children`; inspecting
`expr/src/tree_node.rs:45–105` shows CASE's optional base, every WHEN/THEN pair
and ELSE, both LIKE operands, BETWEEN's three operands, IN's value/list and
null/truth operands all participate. Recursion stops on an unsafe descendant;
a false child cannot be overwritten by a later good child. Neither unreachable
CASE branches nor constant-looking casts can hide an input column or volatile
call. Unknown variants fail proof.

The proof is relative to one inner invocation: literals/parameters/outer
references are fixed there. This preserves valid correlation without claiming
all outer invocations share a value. Composite keys still need every key
column constrained, and nullable-key dependence is still excluded by the
existing owner. The native test following the 23 shapes retains those controls.

Traced both materially different consumers: max_rows at `plan.rs:1450–1457`
feeds scalar-to-join `build_join:379–405`, where unknown means native runtime
single_value, while correlated `logical_plan/invariants.rs:188–194` requires a
sound <=1 proof before execution. The fix repairs the shared producer and
preserves the stricter admission consumer. Three correlated SQL positives at
`subquery.slt:450–480` include PK CASE, GROUP BY-derived unique key and compound
sibling expressions, with exact `33 NULL` unmatched output. The two following
negative CASE queries remain analysis refusals. The peer join journey performs
an actual required-id grouped CASE lookup through a public client and checks
`107 → 108`, `199 → NULL`, so no Bifrost PK DDL prerequisite is being invented.

Cumulative cardinality behavior still uses one native aggregate, ordinary
partial/final state and native registry/codec; the aggregate counts rows rather
than nonnull/distinct values. Its counter remains 0 or 1 before an error; runtime
writers produce only those states. No second cardinality protocol is present.
The rewrite resolves the registered aggregate once per node and uses the same
build_join in filter and projection; proven singleton MAX/LIMIT paths retain
their existing bypass.

## Prior finding closure

- FIND-bifrost-errors-1: CLOSED. Current live owner still retains dynamic
  predicates until readiness, snapshots per execution and assigns each planned
  route once; remote transfer proof remains exact, not just output equality.
- FIND-bifrost-errors-2: CLOSED. The compact corpus replaces broad generated
  workload machinery; no duplicate query oracle or promoted foundational
  DataFusion dependency was found in the cumulative candidate.
- FIND-bifrost-errors-3: CLOSED. Global single_value enforcement remains, and
  FULL/grouping/overflow plus hidden-row-dependence proof repairs are retained.
- FIND-bifrost-errors-4: CLOSED. Required Panics docs remain on the materially
  changed Wyrd mapper/live tests; no new behavioral replacement is needed.
- FIND-bifrost-errors-5: CLOSED. Native proof family and both consumers now have
  positive and negative source-backed coverage; actual Wyrd grouped correlation
  asserts both matching and unmatched outcomes.

## Proof assessment and exclusions

The implementation's reused-client correction matches the source: the peer
join and CASE calls share one API key/client, avoiding duplicate provisioning;
it does not change retries, deadlines or expected rows. The prior unexpected
API-key failure is therefore a harness setup error with a local diagnosed fix,
not evidence that the query assertion was weakened.

Recorded checks are credible for their selectors and assertions. The current
round's evidence does not include a new broad all-language or benchmark run,
and none is claimed. The historical untraced 604-byte reservation observation
has no source-backed recurrence here and is not a new finding. Runtime
multi-row scalar refusal remains opaque 500; the approved detail contract is
for caller planning errors, so changing its status is not required remediation.
Distributed flag-off limitations outside the approved filter/projection/live
query shapes are not a reason to expand this task.

## Proposed findings

Empty. No blocking behavioral, regression, scope or contract defect established.
