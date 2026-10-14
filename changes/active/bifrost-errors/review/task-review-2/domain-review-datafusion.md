# DataFusion / OLAP domain review

## Subject, authority, and coverage

Base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`; candidate
`7d6a1ff95b3dc09315e3af236ab9906b305e11d0` (HEAD unchanged at report time).
DataFusion `5697cfa35192f175bbcd54f59a5fd88527c2bbf5` and distributed fork
`8fac2a978bdcf8e06457238ebfe9c7f35023c123` were inspected from the pinned
Cargo checkouts. DataFusion's complete revision delta is one file:
`physical-expr/src/scalar_subquery.rs`, adding snapshot behavior and tests.
The distributed fork is unchanged. No candidate source was edited.

Authority: approved spec REQ-002/REQ-006; owner direction requiring ordinary
analytical query coverage and no avoidable scalar-query performance regression;
AGENTS.md and agent-rules; spec-driven-development and maintainer-style;
Bifrost design and domain references `datafusion.md` and `olap-serving.md`.
Navigation started with CodeGraph, then independently expanded into the forks.

| Boundary | Source inspected | Assessment |
|---|---|---|
| SQL diagnostics | Cumulative `oracle/mod.rs` mapper delta, planning call site and mapper tests | Narrow logical caller-fault disclosure remains appropriate; no domain finding |
| Session configuration | `resources.rs::OracleSessionShape::apply`; `analytical.rs::planning_session`; `oracle/mod.rs` physical build | Two passes reuse the native optimizer and repair extraction of rewritten equality keys; analytical configuration still disables physical uncorrelated scalars |
| Logical producer | DataFusion `optimizer.rs` rule order; `scalar_subquery_to_join.rs::rewrite/build_join`; `extract_equijoin_predicate.rs::rewrite` | Equality proof is sound, but the rewrite has broader semantic/performance limits below |
| Physical producer/readiness | DataFusion `core/src/physical_planner.rs`, hash join `exec.rs`, `stream.rs`, `shared_bounds.rs`, NLJ pushdown and execute | Hash join waits for complete global build-filter publication before probe polling; NLJ does not produce its own scalar predicate |
| Runtime consumer | Full changed `LiveScribeExec` methods, route selection, fragment construction, lazy read stream | First-poll snapshot is the right boundary; stable planned route assignment followed by narrowing prevents partition-dependent duplication/omission |
| Shared-state and wire | Scalar snapshots; dynamic expression snapshot; hash membership wire encoding; hash join reset and consumer detection | Static scalar metadata preserved; lookup encoding safely widens to true; reuse limits explicitly recorded below |
| Sibling sources | `exec.rs` provider scan/projection, published/hot composition and DataFusion native scan filtering; LiveUnionBoundary | No new evaluator or duplicate query planner; normal DataFusion source filtering remains the sibling mechanism |
| Proof | Remediation evidence table; remote scalar/join test bodies; compact corpus; focused SDK tests | Equality bounded-output proof is useful and deterministic; it does not establish inequalities or cardinality refusal |

## Validated positive execution evidence

The rule order in DataFusion `optimizer.rs:299-311` is
`ScalarSubqueryToJoin`, `ExtractEquijoinPredicate`, then `PushDownFilter`.
A second pass can extract a comparison that the first pass moved into a join.
This is an appropriate shared configuration correction, not a custom Wyrd
planner. The existing comment's microsecond cost estimate is not independently
measured evidence; no benchmark is needed to establish the plan-shape property.

HashJoin creates a probe-side dynamic filter with an initial true expression
(`hash_join/exec.rs:975`). Its Post pass only retains the producer when a
consumer actually holds the expression (`exec.rs:1985-2031`). Wyrd exposes its
predicate via `LiveScribeExec::apply_expressions`, so that consumer detection
includes the live leaf even though it reports the filter unsupported. The
unsupported claim is conservative and retains authoritative operators.

HashJoin constructs the probe stream during execute, but `HashJoinStream`
starts in `WaitBuildSide`, then `WaitPartitionBoundsReport`; build reports await
the shared accumulator's completion (`shared_bounds.rs:456-468`), and only
then does `fetch_probe_batch` poll the probe. Thus first-poll snapshot at
`live.rs:676` sees the completed scalar equality filter. Snapshot in execute
would have been too early. Partitioned filters are globally finalized from
every build partition, including permissive cancellation/unknown handling;
they are not an unsafe local-partition membership subset.

`resolved(partition)` deals the stable planning-selected route list by index
before narrowing. Each route has one partition owner even if different
partitions observe different conservative runtime snapshots. Static planning
pruning remains available from independent conjuncts. Local scalar snapshots
retain metadata and typed nulls and leave the original runtime expression on
the reusable plan. Native hash-lookup serialization produces true because its
map cannot travel; this widens remote work rather than losing matching rows.
Dynamic TopK snapshots may miss later pruning opportunities but remain
conservative; no monotonic-filter correctness regression was found.

The exact remote test at `peer_network/live_subquery.rs:120-176` proves two
opened fragments and two output footer rows, and checks static and scalar
literal values in the fragment. It proves bounded transferred rows for that
equality query; it does not prove that Scribe physically examines only one
row. Its descriptive wording about reads should not be treated as a scan-byte
assertion.

## Proposed findings

### DF-R2-1 — INCORRECT: scalar range predicates still transfer the outer live tail

**Obligation:** prior FIND-bifrost-errors-1 and owner requirement for live
scalar support without avoidable query-path regression. The R1 recommendation
and unit proof explicitly cover a compound scalar `>=` time predicate, not
only equality.

**Changed consumer locations:** `resources.rs:4122` (two-pass configuration),
`live.rs:447-475` (accepting only available pushed predicates),
`live.rs:298-317` (snapshot). **Producer evidence:** distributed
`session_state_builder_ext.rs:27-29` disables physical scalars; DataFusion
`scalar_subquery_to_join.rs:389-391` constructs a join rather than a scalar
expression; `extract_equijoin_predicate.rs:83-150` extracts equality keys only;
`core/src/physical_planner.rs:1615-1746` uses NLJ for no-key joins with piecewise
merge disabled (default false, common config `:1807`). NLJ
`nested_loop_join.rs:841-913` only routes parent dynamic filters and never
creates a filter from its own build result.

**Reachable path:** on a peer-enabled session, use
`SELECT id FROM events WHERE event_time >= H1 AND event_time >=
(SELECT MAX(event_time) FROM pick)`, where pick's one row is near the end of
H3. The scalar rewrite moves a range comparison into a no-equi-key join; two
optimizer passes cannot make `>=` a hash key. It remains an NLJ. Its own
comparison is not a parent predicate over only the events schema, and no
producer emits a resolved events-side dynamic filter. Therefore the leaf sees
only its independent H1 bound; all surviving outer live rows cross the
boundary before the authoritative join decides the range. Swapping which
side NLJ builds does not create the missing producer. The initial rewrite
puts outer rows on the left/build (`scalar_subquery_to_join.rs:389`), and NLJ
collects that side (`nested_loop_join.rs:726`) before probing its right side;
the common equality hash-join swap is not an inequality solution.

**Consequence:** the same unbounded outer transfer diagnosed in FIND-1 remains
for selective scalar inequalities. Results can still be correct. The corpus
and remote journey use equality; the scalar `>=` unit directly installs a
physical ScalarSubqueryExpr and thus bypasses this analytical rewrite.

**Smallest correction boundary:** retain the native hash-join equality fix;
complete runtime scalar comparison production/readiness in the owning
DataFusion execution/rewrite path for the required no-equi-key scalar shape.
The producer must compute the scalar before events/probe dispatch and supply
a conservative native runtime predicate that Wyrd's existing snapshot
consumer can serialize. Preserve SQL cardinality/null rules and authoritative
join/filter. Do not add Wyrd SQL inspection, duplicate evaluation, or globally
flip the distributed safety flag. A conventional scalar execution path with
correct distributed state binding is another possible engine-owned closure;
the present code does not establish that integration, and claiming equality
coverage as closure is not sufficient.

**Focused closure proof:** extend the existing remote fixed-fixture journey
with a selective `>= (SELECT MAX(...))` query using analytical session
configuration, inspect the resolved range literal, and bound outer fragment
footer rows/opened eligible routes separately from scalar-input work. No timing
threshold or benchmark.

### DF-R2-2 — INCORRECT: analytical scalar cardinality is not enforced

**Obligation:** REQ-006 scalar behavior and owner common-query expectation;
normal SQL scalar semantics must not silently become successful set-join
semantics. This is an unmet supported-feature behavior, not a claim that R1
introduced the configuration.

**Evidence and location:** analytical `planning_session` installs the
distributed builder, which forces the flag false. Pinned DataFusion
`common/src/config.rs:1628-1638` explicitly warns that this restores behavior
which "silently produces incorrect results for multi-row subqueries".
`scalar_subquery_to_join.rs:389-391` joins the subquery directly with no
cardinality check. The real scalar execution owner
`physical-plan/src/scalar_subquery.rs:410-430` correctly rejects a second row,
including a second batch, and produces typed NULL for zero rows; that owner is
bypassed on the analytical configuration. Wyrd's new corpus
`query_coverage.rs:188-212` deliberately covers singleton, empty, and NULL but
not multi-row refusal.

**Concrete reachable query:** with the corpus orders,
`SELECT order_id FROM vala.datasets.coverage_orders WHERE amount =
(SELECT amount FROM vala.datasets.coverage_orders WHERE order_id IN (10,11))
ORDER BY order_id`. The inner query has amounts 50 and 30. The analytical
rewrite performs a join and returns matching order ids (10, 11, 15), whereas
scalar SQL must fail because the subquery has two rows. Projection scalars can
also duplicate outer rows. The authoritative retained join cannot correct
this because it is the node applying the wrong set semantics.

**Scope distinction:** the false flag and unsafe rewrite predate this task;
they are not a newly introduced regression. But accepting general scalar
support with singleton/empty/null examples while explicitly disclaiming this
required edge behavior does not close scalar support. This differs from
unrelated existing debt: it is the exact runtime selected for the newly
qualified scalar query forms. The approved spec does not authorize successful
wrong scalar results or restrict scalar SQL to known singleton aggregates.

**Smallest correction boundary:** DataFusion owns scalar cardinality. Ensure
the scalar-to-join path preserves the existing scalar execution owner's
zero/one/multiple-row semantics before any dependent rows can be emitted;
reuse its established semantics rather than a Wyrd consumer guard. Qualify the
serialized execution through the owned distributed fork if the enforcement
node crosses stages. Do not use LIMIT 1, a first-value aggregate, a Wyrd
allowlist, or flag re-enabling without state-binding proof. The inequality
producer and cardinality enforcement should be examined together, since both
originate in replacing a scalar with an unrestricted join.

**Focused closure proof:** one real SDK query against analytical-configured
server must terminally reject two-row scalar input, including input split
across live/published or batches; retain the existing empty/NULL/singleton
cases. A fork-focused scalar rewrite/execution regression should enforce the
same behavior. No benchmark.

## Verification and scope limits

No benchmarks, synthetic load, or heavy test reruns were performed. The
implementation evidence reports passing exact tests and 57 Oracle journeys;
I read their assertions and source paths, not just the reported totals.
The unexplained 604-byte reservation observation lacks a trace and was not
diagnosed here; the system reviewer owns lifecycle investigation.

DataFusion documents that generic reset_plan_states cannot reuse dynamic-filter
plans (`execution_plan.rs:1996-2000`), and HashJoin reset removes its producer.
Wyrd rebuilds retained query plans per attempt; I did not identify a changed
production caller reusing a reset hash-join plan. The scalar reset tests prove
new values for the physical scalar snapshot path, not generic analytical plan
reuse. Therefore this is a proof limit, not an additional finding.

The datafusion-distributed flag also excludes scalar forms in ORDER BY, JOIN
ON, and aggregate arguments; this report does not bundle those as additional
findings because the task's explicit supported forms and tests concern scalar
filter/projection queries. No new public product surface is requested here.

## Overall result

**FAIL.** The equality path and compact query coverage are materially improved,
with correct ownership of runtime snapshots and route partitioning. Prior
FIND-bifrost-errors-1 is only partially closed for analytical scalar predicates;
the two source-backed gaps above require independent validation. Prior
FIND-bifrost-errors-2's harness/dependency concern is closed within this domain.
