# Native DataFusion and OLAP domain review — round 6

Result: **PASS**. No material domain finding proposed.

## Immutable boundary and authority

Reviewed Wyrd `95b350d078226e811ac298230c233bf935975d42` cumulatively
against integration baseline `945a1548c47897dc94583169a6ba04163a73a749`,
and native fork `b8ec4b1d291cd8e371bb88fc76959d91e8503a1d` cumulatively
against `fbe115d2d86bffc15ae1b7c9ecea5e12e731e25b`. Latest native delta
from `8c3f18e98` located the R5 correction; it was not the sole audit scope.
Both HEADs still matched these identities at the end of source inspection.
Distributed fork remains `8fac2a978bdcf8e06457238ebfe9c7f35023c123`.

Authority: approved `spec.md` revision 1 (REQ-002, REQ-006, INV-001), owner
directions and R1–R5 task packets, AGENTS/agent-rules, spec-driven development,
maintainer guidance, Bifrost query authority, and domain DataFusion/OLAP serving
references. CodeGraph was consulted for live-leaf/planning/session ownership;
native source was inspected directly in the named fork and dependency checkout.
This report audits native execution/proof correctness and structural performance;
the separate tenancy reviewer owns the full permission/tenant-boundary audit.

## Coverage and source conclusions

| Domain obligation | Source inspected and conclusion | Proof assessed |
|---|---|---|
| Singleton bounds must be guaranteed | `expr/src/logical_plan/plan.rs:1450–1538`: FULL JOIN with both sides potentially nonempty and grouping-set aggregates return unknown; join products and UNION sums use checked arithmetic. Other branches preserve a bound, reduce rows, or describe exact cardinality. Unknown reaches the existing runtime scalar check. | Existing bound test covers unmatched FULL JOIN, grouping sets, wrappers, ordinary singleton controls and overflowing sum/product. SQL has both filter/projection refusals. |
| Unique-key value must be fixed per inner invocation | `plan.rs:2909–3030`: existing key completeness/nullability logic remains; equality in either orientation contributes a key only if `is_fixed_value` proves its other operand. Input columns cannot acquire proof merely by being wrapped. | `filter_is_scalar_only_for_fixed_key_values` retains dependent arithmetic/cast/UDF negatives, fixed placeholder/outer/stable/immutable controls, composite and nullable controls, and max_rows fallback assertions. |
| Restore ordinary pure correlated composition safely | Same owner explicitly admits CASE, BETWEEN, IN-list, LIKE/SIMILAR TO and eight null/truth predicates; native `Expr::apply_children` (`expr/src/tree_node.rs:46–112`) visits optional CASE base, every WHEN/THEN, optional ELSE, both pattern operands, every IN member and every BETWEEN operand. `Stop` rejects the containing expression; no branch evaluation can hide an unsafe descendant. Volatile scalar functions reject before their argument walk. Opaque/new variants remain false. | 23 shape constructors insert fixed outer/literal and row-dependent/volatile probes in all represented child positions, in both equality orders. Exact existing test is nonempty; SQL adds PK CASE, grouped CASE and compound-family positives, plus dependent/volatile negatives. |
| Preserve sibling correlated admission | `logical_plan/invariants.rs:154–195` consumes `inner_plan.max_rows() <= 1` for nonaggregated correlated scalars; it cannot rely on the uncorrelated runtime fallback. Direct and pure compound OuterReferenceColumn are fixed for one invocation, preserving this consumer without relaxing its admission. Placeholder is a per-execution bound value; Stable uses the native per-query volatility contract. | `subquery.slt:439–489` preserves direct PK and adds exact positive rows including unmatched `33 NULL`; negative inner-count and random CASE remain analysis refusals. |
| Uncorrelated scalar cardinality globally enforced | `optimizer/src/scalar_subquery_to_join.rs:121–205,252–262,363–405`: shared filter/projection build_join adds native single_value only for uncorrelated unknown/>1 inputs. Missing registered aggregate leaves it unrewritten rather than silently joining a set. Known MAX/LIMIT proofs retain their optimization. | SQL flag-off cases cover empty, NULL, duplicate, multi-partition, FULL JOIN, grouping sets, count/abs and dependent CASE; direct aggregate and physical-scalar references provide controls. |
| Partial/final state counts rows, not distinct values | Full `functions-aggregate/src/single_value.rs`: accumulator retains typed value and row count 0/1, rejects a second input row including equal/NULL, and merges partial counts rather than counting non-NULL values. Empty partials contribute zero. Default registration uses the native aggregate registry. No custom plan node, serialization protocol or distributed counting owner. | Four accumulator tests exercise typed empty, NULL, later batches and merged partials; proto roundtrip test decodes a flag-off joined scalar then still refuses duplicate rows. |
| Distributed final count remains global | `distributed_planner/session_state_builder_ext.rs` still disables physical uncorrelated scalars. `inject_network_boundaries.rs:364–395` retains the network coalesce beneath native CoalescePartitions/SortPreservingMerge, with exactly one consumer task; the native aggregate's final merge therefore sees all partial states. | Recorded analytical refusal journey puts one duplicate in published and one in live storage; successful wrong rows would fail its public error-code assertion. Codec tests/pin fence remain intact. |
| Remote value becomes serializable at the right lifecycle point | Full changed `physical-expr/src/scalar_subquery.rs` snapshot returns native Literal with field metadata, fails while pending and leaves original execution state reusable. Wyrd `oracle/live.rs:277–323,420–480,516–569,673–689` accepts dynamic Post filters, snapshots on first poll, and partitions planned routes before further pruning. It retains leader filtering/all-unsupported pushdown rather than claiming Exact semantics. | Existing focused unit asserts pending refusal, static pruning, each route exactly once, distinct reexecution snapshots and encoded literal. Journey checks exact two fragments/two footer rows and both bounds in fragment bytes. |
| Native join readiness precedes probe IO | `resources.rs:4090–4125` uses two optimizer passes so extracted equality becomes a native hash key. Native hash stream starts WaitBuildSide (`physical-plan/src/joins/hash_join/exec.rs:1773`), waits for build and partition-bound delivery before FetchProbeBatch (`stream.rs:689–797`). Thus first-poll snapshots do not race the build for this path. | Existing native SQL EXPLAIN asserts hash join for MAX scalar. Remote literal journey catches missing scalar filter, duplicated dispatch and full-tail transfer structurally. |
| Real public query accepts corrected correlated shape | `peer_network/live_subquery.rs:311–390` reuses one client and existing seed; grouped required id supplies native uniqueness, CASE uses outer id, exact rows include matched 107→108 and unmatched 199→NULL. No Wyrd query-shape guard or custom correlated executor. | Recorded focused journey gives old-pin planner refusal and current-pin exact rows. Existing join assertion remains. |
| Error handling stays at appropriate layer | `oracle/mod.rs` cumulative mapper distinguishes SQL/logical caller faults from opaque physical/general execution failures; scalar runtime refusal stays execution failure. Native engine correction does not expose engine internals or introduce a new public contract. | Existing exact mapper test and multi-language/MCP caller-error journeys remain in cumulative source/evidence. |

The native functional-dependency producers were also inspected: FULL JOIN drops
dependencies (`common/src/functional_dependencies.rs:373`); grouping-set
aggregate schema handling drops unsound unique keys; ordinary nonnullable
grouping yields Dependency::Single. The corrected Filter proof therefore does
not reintroduce the FULL JOIN/grouping-set bypass through a sibling authority.

## Root-cause and reuse assessment

The latest correction is at the shared producer of the bound, not a special
CASE guard at the correlated analyzer or a Wyrd exception. It preserves both
required consequences of that same proof: safe skip of uncorrelated runtime
validation, and valid correlated admission. Native child traversal avoids
another independent child-enumeration implementation. A general constant
evaluator is not interchangeable: this proof allows outer references fixed
per invocation and does not evaluate SQL or enter nested plans. The closed
positive policy prevents unknown forms from silently becoming proof.

The older remote predicate loss and caller error mapping concern different
owners and remain resolved in cumulative source. single_value uses the native
AggregateUDF/partial-final/codec path; live dispatch uses the existing signed
fragment owner and query bindings. No replacement workflow, new runtime,
public option, extra protocol or generated workload is introduced.

## Performance and verification limits

The proof walk is planning-time, linear in inspected expression size and
short-circuits unsafe descendants. The correction performs no row IO, adds no
runtime work to sound restored correlated lookups, and leaves native hash
readiness, snapshot pruning and disjoint route ownership unchanged. Unknown
uncorrelated cardinality pays the required native constant-state aggregate;
known singleton MAX/LIMIT paths bypass it. These are source/plan conclusions,
not a latency or throughput measurement.

Assessed the appended R5 exact-command/evidence table against actual selectors,
queries and assertions: exact proof unit, one SQL file, expr 265, optimizer 886,
single_value 4, proto library 17, three remote journeys, corpus, two focused
Wyrd units and four codec tests, plus recorded formatting/lint/generated/boundary
checks. The proto-library count is not represented here as a fresh run of all
265 external proto cases; prior round evidence supplies that cumulative proof.
The R5 packet explicitly records no all-520 SQL rerun. No new test/build/lane was
run by this reviewer, and recorded passing commands are assessed evidence,
not independently reexecuted results. No benchmarks or synthetic host load.

The one-client change addresses the documented duplicate API-key provisioning
failure at its fixture cause; assertions, timeouts and retry behavior were not
weakened. Stable functions rely on correctly declared native volatility, as
the rest of DataFusion does. Opaque expressions remain unproven as approved;
this does not claim all future optimizer defects are impossible.

## Proposed findings

None. FIND-3's unsound-bound defect and FIND-5's pure correlated-expression
regression are closed within this domain on the inspected candidate; prior
literal shipping and normal query coverage mechanisms remain intact.
