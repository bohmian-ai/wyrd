# Round 6 — independent invariant review

**PASS. No material proposed findings.**

## Immutable subject and method

Reviewed Wyrd `945a1548c47897dc94583169a6ba04163a73a749` →
`95b350d078226e811ac298230c233bf935975d42`, with original base
`526355fb8c58dae6d67d516f98dddcd5c02b0e79` and the prior remediation
packets retained as history. Native cumulative fork range is
`fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b` →
`b8ec4b1d291cd8e371bb88fc76959d91e8503a1d`; latest delta from
`8c3f18e9806ae23e8840a710af775c058dfad66a` locates R5 closure. Both HEADs
still matched these identities at the final source check.

Authority: approved revision-1 spec, owner directions, AGENTS and agent-rules,
spec-driven-development, maintainer guidance, applicable Wyrd design/doctrine
and Bifrost query/authority/execution sections, and prior verdict/remediation
packets. No original task existed. References in the inherited specification
to unrelated example-journey requirements do not introduce those capabilities
into this split task. This is a fresh invariant role; no current discovery
reports were read. CodeGraph was consulted for live-leaf/error-map and
query/authorization navigation before targeted source reads.

## Producer, consumers and lifetime trace

### One-row proof and both consumers

Native `expr/src/logical_plan/plan.rs:2909–3020` retains one `Filter::is_scalar`
owner. It requires a nonnullable, single-valued functional dependency covering
the full output, a conjunction containing every key column, and equality
against an invocation-fixed value. The opposite operand in either equality
order goes through the same positive classifier. Partial composite keys,
nullable unique keys, OR, and column-to-itself comparisons do not acquire a
proof. No independent Wyrd classifier or correlated-analyzer exception was added.

`is_fixed_value` accepts literals, parameters and typed outer references as
leaves. Pure operators/casts/functions and the R5 pure family use native
`Expr::apply_children`. The native implementation in `expr/src/tree_node.rs:45–105`
visits CASE's optional base, every WHEN/THEN pair and optional ELSE, all three
BETWEEN operands, the IN tested value and every list item, LIKE/SIMILAR TO
value and pattern, and the null/truth operand. A volatile function returns
false before visiting arguments. Any descendant false propagates Stop and
the classifier accepts only Continue. Input columns, session variables,
subplans, aggregates, windows, higher-order/lambda/unnest and unlisted forms
remain unproven. There is no evaluation or optimizer recursion in this proof.
Stable/immutable function classification follows the existing native function
volatility contract; no external-function purity guarantee is invented.

Schema dependencies come from existing native owners, including
`common/src/functional_dependencies.rs:419–510` for aggregate keys;
`plan.rs:4370–4405` does not construct ordinary grouping uniqueness for grouping
sets. FULL JOIN dependency handling drops the dangerous proof rather than
passing a one-side key through an expanding join. This matters because
`LogicalPlan::max_rows`, `plan.rs:1450–1539`, promotes a successful filter proof
to Some(1).

The two directly relevant sibling consumers were traced separately:

- `expr/src/logical_plan/invariants.rs:165–194` uses a proven bound for
  nonaggregated correlated scalar admission. An outer reference is fixed for
  one inner invocation, so compound outer CASE retains valid admission; it
  need not have one value across all outer invocations.
- `optimizer/src/scalar_subquery_to_join.rs:362–405` uses the decorrelated
  bound to decide whether an uncorrelated scalar needs native `single_value`.
  Unknown/>1 adds validation in the shared filter/projection rewrite. A hidden
  inner-row column or volatile descendant cannot now bypass that check merely
  by appearing below an allowed pure node.

The latest correction therefore repairs the existing producer used by both
consumers. It neither weakens correlated admission nor relies on runtime
validation being available where it is not.

### Runtime count and bounded operators

`functions-aggregate/src/single_value.rs` owns the value and 0/1 row count.
Empty batches/states contribute no rows; one NULL still counts as a row; a
second equal or NULL row fails. Native partial states include the count and
the final aggregate merges counts across partitions, so worker separation
does not turn multiple rows into independent accepted scalars. Default
aggregate registration and the native codec retain function identity; no
new execution node or protocol exists. Existing proto refusal-after-roundtrip
coverage protects this seam.

`max_rows` remains conservative for nonempty FULL joins and grouping sets;
join products and union sums use checked arithmetic. Other inspected branches
preserve or reduce rows, have an exact finite bound, or return unknown. The
MAX/global aggregate and LIMIT singleton controls still avoid unnecessary
validation. No new bypass was found in this cumulative range.

### Runtime predicates, route ownership and errors

`physical-expr/src/scalar_subquery.rs:172–188` snapshots a populated scalar
to a literal with its field metadata; pending state returns an error. The
stored runtime expression is retained, and `ScalarSubqueryExec::reset_state`
clears its shared results. `physical-plan/src/scalar_subquery.rs:214–242`
waits for the shared subquery future before executing its main input.

Wyrd `oracle/live.rs:298–317,441–476,533–557,673–690` retains static Pre and
dynamic Post pushdown, advertises unsupported filters so upstream owners remain
authoritative, snapshots on first stream poll and deals the planned route list
across partitions before narrowing each share. Different snapshots cannot
reassign a route between partitions. Native hash-build readiness supplies the
analytical equality value before the probe is polled; `resources.rs:4090–4123`
uses two logical optimizer passes so pushed equality becomes a join key.
The distributed physical-scalar flag remains disabled. Dynamic snapshots are
conservative optimization inputs; retained leader/join predicates still own
result correctness. Stream-owned execution bindings and cancellation ownership
are unchanged.

Wyrd `oracle/mod.rs:3010–3037,3735–3820` maps typed logical caller faults
through wrappers/collections to safe planner detail; physical planning uses
fixed repair text and runtime failures retain generic/catalog mapping. The
source of suggestions is restricted by table authorization before provider
registration: `prepare_query_attempt` → planner `pin_cut` →
`read_and_materialize` → `authorize_resolved_tables` precedes materialization
and planning. SELECT reference collection includes joins, CTEs and subqueries.
The public transport/error catalog is reused, not duplicated.

## Reconciled obligation matrix

Verification entries below assess recorded execution and actual assertions;
this reviewer did not execute builds or tests.

| Obligation | Implementation evidence | Verification evidence | Result |
|---|---|---|---|
| REQ-002 unknown column/table/function, type and ambiguity produce actionable planning 4xx | Shared logical mapper and authorized session boundary in Oracle | Rust/Python/TS query_bifrost and MCP assertions inspect codes and planner suggestions/detail across single/join/subquery forms; focused mapper test | PASS |
| REQ-002 internal failures remain opaque 5xx | Physical fixed detail; runtime general/catalog mapper unchanged | Mapper internal/wrapper/resource cases; no new internal text projection | PASS |
| INV-001 and applicable AC-002 authorization negative | Resolved-table authorization before provider/source work; signed/identity-bound fragment execution unchanged | Scoped unauthorized-table journeys assert forbidden and absence of both misspelled and real protected column names | PASS |
| REQ-006 live scalar/IN/EXISTS and managed run_id correlation | Native DataFusion planning and common live leaf | First-class Rust/Python/TS and MCP live fixture/expected rows versus joins; managed run_id fixture now has distinct groups | PASS |
| R1 selective analytical scalar shipping and re-execution | Native literal snapshot, Pre/Post leaf, first-poll resolution, immutable retained predicate and partition-first dealing | Exact snapshot/static-pruning test; remote journey asserts exact result, two fragments, two footer rows and both literal times | PASS |
| R2 global scalar cardinality | Shared filter/projection rewrite plus native single_value partial/final merge | Native zero/one/equal/NULL/batch/merge controls, proto roundtrip, remote two-row filter/projection journey | PASS |
| R3 bound soundness | FULL/grouping conservative unknown; checked arithmetic in native max_rows | Exact native bound controls and remote FULL/GROUPING SETS refusal cases retained | PASS |
| R4 hidden input dependence cannot grant a one-row proof | Positive fixed-value classifier at Filter owner | Native a=b+0/count/abs controls and SQL/peer projection/filter refusals retained | PASS |
| R5 preserve all selected ordinary pure correlated forms | Positive family plus native complete child traversal | 23-shape owner matrix with outer/literal and input/volatile probes in both equality orders; native PK/grouped CASE and compound sibling SQL exact rows/unmatched NULL | PASS |
| R5 public grouped required-id CASE proof | Existing live join journey reuses seed/client and adds correlated grouped lookup | Exact `(107,108)` and `(199,NULL)` expectations; recorded three focused remote journeys green | PASS |
| Human common-query coverage addition including joined CTE | Fixed two-table mixed published/live Rust corpus, no generated oracle | Exact expected projection/group/HAVING/inner/left/scalar/IN/EXISTS/NULL/CTE/window rows; focused corpus selector recorded 1 pass | PASS |
| AC-001 strongest shipped-surface journey proof | Real SDK/server fixtures, real MCP transport; focused analytical peer seam | Existing first-class/MCP evidence retained; R5 peer query shows old-pin analysis refusal then new-pin exact rows | PASS |
| AC-003 identities and focused checks | Cargo pin/lock and codec revision all b8ec4b1d2; no contract churn | R5 exact native proof test + subquery file; expr265/optimizer886/single_value4/proto17; Wyrd selectors2/4/3/1 and recorded fmt/lints/codegen/deps/tenancy/diff checks | PASS |
| Non-goals, performance and existing ownership | Native Filter/aggregate/rewrite/snapshot mechanisms; existing peer journey and compact corpus | No new framework/proof type/protocol/production test dependency; structural transfer proof, no benchmark claim | PASS |

## Prior finding closure

- **FIND-bifrost-errors-1:** closed by native execution-time snapshot and the
  analytical equality/hash path, preserving retained authoritative filters and
  deterministic route ownership. The original issue is not merely hidden by
  post-transfer filtering.
- **FIND-bifrost-errors-2:** closed; compact fixed corpus replaces the broad
  workload/oracle machinery. Test-only fixture extensions reuse existing owners.
- **FIND-bifrost-errors-3:** closed; runtime native count plus repaired FULL,
  grouping and input-dependent filter proof prevents diagnosed false-bound
  bypasses. R5 adds no unsafe descendant escape.
- **FIND-bifrost-errors-4:** closed; changed mapper/MCP test Panics docs remain.
- **FIND-bifrost-errors-5:** closed; sound pure outer expressions reach both
  native proof consumers and the public grouped-key CASE journey. The native
  child walk avoids one handwritten omission per expression family.

## Findings and limits

Proposed finding ledger: **empty**. No new product decision, Wyrd downstream
guard, analyzer weakening or constancy framework is necessary for this task.

No tests, benchmark, synthetic load or heavy lanes were run during this audit.
Recorded execution is assessed against source/assertions rather than treated
as proof of every SQL form. The task does not make future optimizer defects
impossible; the diagnosed invalid proof construction is prevented at its owner.
The old untraced memory-reservation observation remains unexplained and supplies
no independently diagnosed new finding here. Runtime scalar refusal's opaque
500 remains outside the approved caller-planning-detail contract. These limits
do not invalidate the source-backed closure above.
