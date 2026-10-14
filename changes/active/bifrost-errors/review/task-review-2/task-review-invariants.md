# Independent invariant review

## Subject and result

Base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`; candidate
`7d6a1ff95b3dc09315e3af236ab9906b305e11d0`. DataFusion is
`5697cfa35192f175bbcd54f59a5fd88527c2bbf5`, distributed fork remains
`8fac2a978bdcf8e06457238ebfe9c7f35023c123`. HEAD was checked against the
candidate. Review is cumulative, with the first review ledger and remediation
used as closure hypotheses. No original task existed.

**Overall: PASS for the bounded task and remediation outcomes. Proposed
blocking findings: none.** The performance evidence establishes selective
equality-scalar pushdown on the distributed-configured path, not an exhaustive
performance guarantee for every scalar syntax. The limits below must not be
represented as newly verified behavior.

## Authority and source coverage

Reviewed the supplied AGENTS rules, agent-rules, spec-driven-development,
maintainer-style, Wyrd design/doctrine, Bifrost design, the reference router and
the DataFusion/OLAP references. CodeGraph was the initial source navigation.
Read the cumulative diff and traced materially changed production owners and
their consumers, with particular attention to `live.rs`, `resources.rs`,
`oracle/mod.rs`, `analytical.rs`, fragment encoding/dispatch, server footer
instrumentation, shared query fixtures, and the new and retained journeys.

The pinned DataFusion revision differs from the previous revision only in
`physical-expr/src/scalar_subquery.rs` (115 added lines including tests). Its
snapshot implementation and tests were read, as were the existing dynamic
filter, hash-join producer/stream ordering, nested-loop forwarding and reset
contracts. No second evaluator, serialization format, physical planner or
distributed fork modification entered this candidate.

## Producer, state, and consumer trace

### Interactive scalar results

DataFusion owns `ScalarSubqueryResults`; `ScalarSubqueryExec` fills the result
before executing the main input. `ScalarSubqueryExpr::snapshot` obtains that
value, fails with a typed internal DataFusion error if absent, and constructs
the installed `Literal::new_with_metadata` using `FieldMetadata`. The fork
tests cover ready, pending, typed NULL, metadata, recursive composition and a
changed value after clearing the slot. The Wyrd physical plan retains the
original expression; only its execution-local leaf clone receives the literal.

`LiveScribeExec::execute` creates a stream without opening fragments.
`LiveFragmentRead::into_stream` checks the bound grant and obtains its live
dispatcher, then `resolved(partition)` snapshots before any route is opened.
An error therefore aborts this request's stream before fragment IO. Fragment
encoding remains the ordinary DataFusion filter over the identity-bound remote
placeholder. Neither result state nor tenant authority is sent as a new wire
contract.

### Distributed-configured equality scalar results

`planning_session` (`analytical.rs:6645-6678`) retains the distributed planner
and its disabled physical-uncorrelated-scalar flag. `OracleSessionShape::apply`
(`resources.rs:4108-4125`) raises the existing logical optimizer budget from
one pass to two; the existing equijoin extraction and filter rules can now
produce a hash-join key from the rewritten equality. This is shared session
configuration, not a separate planner or query-shape heuristic. The leader,
follower and stage consumers continue using the existing session owner.

DataFusion's hash join owns the dynamic filter and its complete build-side
state. `hash_join/stream.rs:670-779` transitions through build collection and,
where applicable, waits for all partition bounds before polling the probe.
`hash_join/exec.rs` calls the probe's `execute` before that readiness point,
which is why moving Wyrd's snapshot from `execute` to first stream poll is
substantive and correct. Wyrd now accepts Post-phase dynamic filters
(`live.rs:441-476`) but reports them unsupported to the leader, preserving the
join as the correctness authority. A hash-table lookup whose codec degrades to
`true` is conservative: it can reduce pruning benefit, never remove a row the
join requires.

The implementation does not assume every dynamic producer completes before
the leaf polls. TopK and other incremental filters may still be their initial
or earlier conservative predicate. Taking one immutable snapshot loses later
pruning opportunities but leaves their retained producing operator responsible
for correctness. No per-row, batch or route scalar evaluation was added.

### Partition and route ownership

Planning-time pruning keeps independent conjuncts that can snapshot without
pending-state errors (`live.rs:463-474`). The original pending expression is
still retained in the full predicate. `resolved` first assigns planned route
index `i` to partition `i % partition_count`, then narrows only that partition's
share (`live.rs:298-317`). Thus different runtime snapshots cannot redistribute
a route to a second partition or cause a route to fall between partition
assignments. Each partition uses its same immutable snapshot for subsequent
route pruning and fragment filters.

Dealing before narrowing differs from the private task's prescribed order,
but preserves the stronger route-ownership invariant under independently
polled dynamic filters. There is no public or persistent behavior decision to
revise. The unit proof checks the union of both partition shares and exact
eligible route membership, then a changed scalar value on the same planned
leaf.

### Authorization, error mapping and observations

Logical caller-fault mapping accepts only SQL/Plan/Schema/NotImplemented leaves
under the intentionally narrow Context/Diagnostic/Shared/Collection wrappers.
Resource errors take priority and retain their stable mapping; physical-plan
details remain fixed and internal failures retain 5xx mapping
(`oracle/mod.rs:3735-3802`). Existing authorization and table registration
remain upstream of logical planning, rather than delegating authority to
DataFusion. The retained per-surface negative journeys exercise redaction.

Fragment assignments still carry the same authenticated physical table,
writer fence, schema fingerprint and immutable route. New output counters use
the actual emitted `NativeOutputTally` footer rows, not estimated rows. The
last plan and row counters are behind `test-support`; production gets neither
plan retention nor its mutex. The test uses a dedicated Scribe after seeding,
so the two query fragments are attributable to the query under assertion.

## Acceptance matrix

| Obligation | Implementation evidence | Verification evidence available | Result |
|---|---|---|---|
| REQ-002 actionable logical-planning 4xx, safe internal failures | `oracle/mod.rs` narrow caller-fault traversal and distinct physical mapper | Exact mapper unit plus Rust/Python/TS/MCP negative journeys | PASS |
| REQ-006 scalar, IN, EXISTS and run_id-correlated live subqueries | Snapshot owner, unchanged generic DataFusion rewrites, managed run-id fixture and focused journeys | Focused cross-surface journeys; Rust 9 query_bifrost tests; Oracle lane 57/57 recorded | PASS |
| INV-001 tenant isolation | Existing authorized session/cut and unchanged identity-bound fragment assignments | Redaction journeys; recorded tenant-isolation lane | PASS |
| R1 criterion 1 ready/pending/metadata/recursive/NULL/reset values | Pinned `ScalarSubqueryExpr::snapshot`; original leaf expression retained | Fork 10 scalar tests; Wyrd direct changed-value unit | PASS |
| R1 criterion 2 materialized selective scalar and bounded remote output | Hash-join readiness plus first-poll snapshot; ordinary codec; footer tally instrumentation | remote_scalar_subquery_ships_as_a_literal: expected result, 2 fragments and 2 footer rows, both literals inspected | PASS |
| R1 criterion 3 static pruning and exact partition ownership | Pre-conjunct pruning; immutable planned-route assignment before runtime narrowing | static-pruning unit and remote hour-route assertions | PASS |
| R1 criterion 4 public behavior remains covered | Existing Rust/Python/TS/MCP focused journeys retained | Recorded focused journey results; no signature/wire change | PASS |
| R1 criterion 5 remove generated qualification machinery/dependency promotion | Support-workload module and broad suites absent in cumulative candidate; no new normal testing DataFusion dependency | Source/manifests and recorded check:deps | PASS |
| R1 criterion 6 compact fixed mixed-state common-query corpus including outer CTE join | `query_coverage.rs`: small two-table fixtures, explicit rows, 16 cases; first set published before second set written | Exact common_queries_answer_over_published_and_live_rows recorded PASS | PASS |
| R1 criterion 7 representative remote live join | `peer_network/live_subquery.rs` separate remote_live_join_returns_its_rows | Exact journey recorded PASS | PASS |
| R1 criterion 8 focused quality/contract checks | Manifest and codec revisions agree; all instrumentation consumers updated | fmt, lints, codegen, deps, tenant-isolation and diff check recorded zero exit | PASS |
| No new scalar evaluator, allowlist, planner, broad harness, public contract or benchmark | Cumulative source/dependency diff | Static review; no benchmarks run | PASS |

The original spec retains unrelated carried-over AC-001/AC-002 references to
Jev/traceparent/CLI requirements from its parent packet. They create no
reachable obligation for this split task's write set.

## Prior finding closure

- **FIND-bifrost-errors-1:** Closed for the diagnosed dropped-predicate defect
  and requested remote selective equality proof. Interactive snapshots use the
  DataFusion owner. Distributed-configured equality uses its existing hash
  join producer, whose readiness now matches the Wyrd snapshot boundary. The
  result-only escape hatch is replaced with exact fragment and output-row
  observations.
- **FIND-bifrost-errors-2:** Closed. Generated three-table data/oracle framework
  and ordinary dependency promotion are gone; the owner-directed SQL corpus
  uses two small fixtures and explicit expected answers, with separate narrow
  remote tests. This adds requested coverage without rebuilding an oracle.

## Limits and non-blocking notes

1. No independent executable reruns or benchmarks were performed; other
   worktrees are active. Recorded exact commands and source were evaluated,
   rather than treating the chat summary as verification.
2. The recorded earlier 604-byte reservation observation has no established
   traced cause. A later 57/57 Oracle run is useful evidence but does not explain
   that observation. No reservation or cancellation weakening is visible in
   this candidate; do not invent a diagnosis or call the leak fixed.
3. The pinned DataFusion config docs (`common/src/config.rs:1626-1640`)
   explicitly warn that disabling physical scalar subqueries loses multi-row
   refusal and scalar support in certain expression positions. The distributed
   flag predates this candidate. The new corpus checks one-row, empty and NULL
   scalars, not multi-row refusal. This is a real existing engine limitation,
   not an independently introduced task finding.
4. Non-equality scalar filters can remain nested-loop joins in the
   distributed-configured path. The pinned NLJ pushdown hook
   (`nested_loop_join.rs:840-921`) forwards parent dynamic filters but creates
   none from its own join condition. The equality remote proof cannot be used
   to claim arbitrary comparison-scalar runtime pushdown. This path pre-exists
   the remediation; no new incorrect pruning or row-loss path was found.
5. DataFusion's `reset_plan_states` explicitly excludes plans with dynamic
   filters (`physical-plan/src/execution_plan.rs:1988-2009`). Wyrd production
   has no caller of that utility in this reviewed path. The scalar slot
   reset/re-execution test is valid for its stated owner; it does not establish
   general dynamic-join plan reuse, and this review does not require repairing
   that unsupported adjacent lifecycle.

No proposed new blocking finding survives this invariant review's scope and
reachability checks.
