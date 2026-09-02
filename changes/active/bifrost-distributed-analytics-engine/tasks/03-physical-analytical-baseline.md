---
id: BIFROST-R4-T03-PHYSICAL-BASELINE
title: Qualify the minimal cross-process Analytical operator baseline
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 4
depends_on:
  - BIFROST-R4-T02-QUERY-ENVELOPE
  - BIFROST-R4-T02A-RUNBOOK-AUTHORITY
requirements: [REQ-003, REQ-005, REQ-007, REQ-009, REQ-011]
invariants: [INV-002, INV-003, INV-004, INV-005, INV-007, INV-008]
acceptance: [AC-001, AC-002, AC-004, AC-005, AC-007, AC-008]
parent_task: BIFROST-R3-T1-INACTIVE-CLOSEOUT
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# Minimal physical Analytical baseline

## Outcome and value

One exact three-Oracle/one-Scribe process journey proves the representative raw
SQL join/group query across real peers, including remote pushdown, exchange,
native DataFusion join and aggregation, a real `SortExec` spill, exact results,
interchangeable coordinators, private peer isolation, bounded production
telemetry, deadline-bound settlement, and zero retained ownership. This task
does not broaden compatibility or activate public routing.

Required execution skill: `$wyrd-implement`.

## Current-state amendment

- **Retain:** Task 2's closed supported-plan predicate and graph-lifecycle
  owner; `OraclePhysicalExtensionCodec` for Wyrd remote-scan and tenant-tripwire
  extensions only; DataFusion's native join, aggregate, sort, and exchange
  codecs; the existing passing raw-SQL join/group journey; the
  `BifrostProcessCluster` three-Oracle/one-Scribe topology; process-owned spill
  directories; private listener probes; and the mixed and role-separated
  deployment manifests.
- **Delete:** the stale follower-join rejection claim, join/aggregate codec
  extension work, separate join and spill journeys, the six-replica physical
  fixture, the one/two/three/six listener topology loop, retry telemetry, and
  any requirement for profile-label-only deployment manifests.
- **Unfinished:** move the existing physical proof onto
  `BifrostProcessCluster`, make its data and coordinator use asymmetric, force
  the existing ordered join/group query to spill under the fixed production
  resource calculation below, retain the identified output `SortExec` metrics,
  expose exact result and production metric evidence through the existing test
  control response, and bind distributed metric settlement to Task 2's absolute
  graph deadline.

## Owners, scope, consumers, and non-goals

Primary owners are `oracle/exec.rs` and `oracle/mod.rs` for the existing
rewritten-plan metric fold, Task 2's `AnalyticalSupervisor` graph lifecycle for
deadline and cleanup settlement, `oracle/telemetry.rs` for production physical
metrics, and `wyrd-testing`'s existing process-cluster control and Oracle
journeys. Task 4 consumes the qualified handle and Task 2 predicate.

Do not modify `OraclePhysicalExtensionCodec` for native DataFusion join,
aggregate, sort, or exchange nodes unless a separate failing test demonstrates
a Wyrd extension defect. Do not add per-graph scratch directories, a second
scratch manager, deployment profiles, windows, correlated subqueries,
deduplicating sets, UDAFs, join families, shuffle infrastructure, an optimizer,
a scheduler, a dependency patch, retry machinery, or production activation.

## Ordered implementation scenarios

### Scenario 1 — One representative process journey proves the physical baseline

**Behavior.** The same raw SQL statement performs a multi-input inner equi-join,
fixed-width grouped aggregate, and its existing `ORDER BY` across distinct
Oracle processes. Under the fixed production-derived 256 MiB query grant below,
its native `SortExec` spills and completes correctly. The
journey executes once through Oracle 0 and
once through Oracle 1, proving either process can coordinate while every
selected non-coordinator follows; peer services remain absent from public
listeners and reachable only through the authenticated private plane. Maps
REQ-003, REQ-005, REQ-007, REQ-009,
INV-002, INV-003, INV-004, INV-005, AC-002, AC-004, AC-005, AC-008.

**RED.** Replace the six-replica
`pg_inactive_analytical_raw_sql_executes_join_and_partial_final_aggregate_on_followers`
proof with
`peer_network::analytical::inactive_baseline_executes_join_group_spill_and_interchangeable_topology`.
Start exactly three `ProcessNodeTarget::Oracle` children and one
`ProcessNodeTarget::Scribe`. Give every child the existing production resource
calculation through `WyrdTestServerBuilder::with_system_resources_for_test`:
512 MiB process memory, four effective CPUs, and the existing 4 GiB scratch
capacity/availability. With one enabled role, the required 256 MiB unmanaged
reserve leaves the required 256 MiB role floor and no elastic memory. An idle
two-slot Analytical query therefore receives exactly the production
`ORACLE_PARTITION_MEMORY_BYTES` cap of 256 MiB; assert that grant from the
production resource snapshot before execution.

Seed the left table with IDs `0..400_000` and the right table with IDs
`0..300_000` over the existing bounded Scribe path and short key, using fixed
100,000-row requests. Add only `start_id` to the existing test-only
`IngestRows` control request so those ranges do not restart; do not add another
ingest route or wide ingest fixture. Every request remains below the
131,072-row ceiling. Execute this exact shape,
preserving the existing join/group/ordering proof while keeping non-spilling
join and aggregate state narrow:

```sql
SELECT LPAD(CAST(l.id AS VARCHAR), 6, '0') || REPEAT('x', 1018) AS filter_key,
       COUNT(*) AS matched
FROM vala.bifrost.<left> AS l
JOIN vala.bifrost.<right> AS r ON l.id = r.id
GROUP BY l.id
ORDER BY filter_key
```

The projection after the `Int64` group key creates 300,000 distinct, exactly
1,024-byte ASCII sort keys without making the hash aggregate retain those wide
keys. The output sort has a conservative fixed-width lower bound of
310,800,004 bytes: 307,200,000 key bytes, 1,200,004 UTF-8 offset bytes, and
2,400,000 non-null `Int64` count bytes. This exceeds the asserted 268,435,456-
byte query grant by 42,364,548 bytes before DataFusion sort overhead. Refresh
every child, then run the same join/group/`ORDER BY` SQL first on Oracle 0 and
then Oracle 1.

Checkpoint every process-owned `root()/spill` directory and production metric
snapshot before the first query. For each execution, hash every decoded output
row as the length-prefixed UTF-8 key followed by the little-endian `Int64`
count; assert 300,000 rows, count `1` for every row, increasing keys, and the
same SHA-256 digest independently produced by the parent fixture generator.
Sum `RecordBatch::get_array_memory_size()` over those decoded batches and
assert the measured sorted output is at least the calculated lower bound and
greater than the reported query grant. `SortExec` preserves this schema and row
set, so the cumulative decoded output is the same logical working set that
entered the identified output sort without adding a test execution node. Also
assert distinct child PIDs, both
non-coordinator Oracles' scan or GraphLease activity, and positive exchange
bytes/batches.

Identify exactly one output `SortExec` by its output schema (`filter_key`,
`matched`) and ordering (`filter_key ASC NULLS LAST`) while traversing ordinary
children and distributed `Stage::Local` edges. Fail on zero or multiple matches.
Assert the aggregate groups only on the narrow `Int64` ID and the
`HashJoinExec` build child is the right table's projected `id` input, so neither
non-spilling operator retains the projected wide sort keys.
Read only that node's retained `MetricsSet` and assert its `spill_count`,
`spilled_bytes`, and `spilled_rows` are positive; whole-plan aggregate counters
cannot satisfy these assertions. The successful complete digest after those
positive metrics proves spill read. After each terminal, assert every
process-owned spill directory
has returned to its pre-query entry/byte baseline and every live graph, lease,
task, cache, exchange, pool, scratch, slot, and physical-current gauge is at its
pre-query baseline. Probe both coordinators' public gRPC listeners and private
peer listeners: peer services must be `Unimplemented` publicly and reachable
only through the existing authenticated private-plane probe. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::analytical::inactive_baseline_executes_join_group_spill_and_interchangeable_topology)' --run-ignored=all"
```

It initially fails because the existing passing proof uses
`WyrdTestCluster::six_capacity`, returns only row counts through the process
control response, injects no fixed production resource snapshot, aggregates
metrics across the whole plan, and does not couple an identified output sort,
spill, coordinator interchangeability, or listener isolation to that query.

**GREEN.** Move the existing SQL and result assertions into
`peer_network/analytical.rs`; delete the old six-node test after the process
journey carries strictly stronger evidence. Extend the existing
`ExecuteInactiveSql` control request/response only as needed to return the exact
ordered fixed-width result and the production physical metric snapshot already
owned by the executed plan. Add the fixed `SystemResourceSnapshot` values to
the existing `BifrostProcessCluster` child configuration and pass them through
`WyrdTestServerBuilder::with_system_resources_for_test`; all grant and floor
math remains in `BifrostRuntimeResources`. Extend the existing bounded fixture
request only with the fixed starting offset above.

At physical-plan start, retain the unique output `SortExec` metric set directly,
following `ForgeRewrite::sorted_stream`: hold the matching native node long
enough to call `sort.metrics().unwrap_or_default()` after `execute_stream` has
registered its metrics, then read `MetricsSet::spill_count`, `spilled_bytes`,
and `spilled_rows` after lifecycle settlement. Extend the existing distributed
tree walk only to cross `Stage::Local` edges and locate that exact sort; do not
generalize it into a spill registry or report whole-plan spill totals. Read
spill baselines directly from each `ProcessNode::root().join("spill")`; do not
add a scratch abstraction or let test code create/remove spill files. Reuse the
existing peer probes and live ownership inspections. Native join, aggregate,
sort, and exchange execution remain DataFusion-owned.

**REFACTOR.** Keep the process control protocol test-only and bounded. It may
project completed production results and metrics but may not synthesize
physical evidence or duplicate query lifecycle behavior.

### Scenario 2 — Metric folding obeys the graph deadline and settlement order

**Behavior.** Follower metrics are folded through the existing rewritten
distributed plan as one descendant of Task 2's graph lifecycle. The same
absolute deadline covers execution, metric arrival, spill cleanup, and terminal
settlement. Missing metrics cannot hang settlement or produce a success
terminal. Maps REQ-007, REQ-011, INV-004, INV-005, AC-001, AC-004, AC-007.

**RED.** Add
`oracle::analytical_supervisor::tests::distributed_metrics_settle_within_the_graph_deadline`.
Use the existing deterministic graph gates and paused Tokio time to hold
follower metric completion beyond the immutable deadline. Assert the lifecycle
remains owner of the fold, the deadline selects the existing failed-terminal/
`Draining` route, no successful release is published, readiness is false, and
shutdown still observes and joins the graph. Release metrics before the
deadline in the companion case and assert spill metrics are folded before
scratch verification, success publication, and graph removal. Exact:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical_supervisor::tests::distributed_metrics_settle_within_the_graph_deadline)'
```

**GREEN.** Remove the unbounded await from the result-stream tail in
`Oracle::with_distributed_scan_metrics`. Register the existing
`record_distributed_scan_metrics` future, physical plan, and scan-metric sink as
one descendant of Task 2's existing supervisor graph lifecycle. That lifecycle
awaits the fold under its existing absolute deadline, then folds the native
spill metrics and performs its existing process-owned scratch cleanup check
before publishing success and removing the graph. Rewrite/metric failure or
deadline expiry uses the existing distributed execution failure and
failed-terminal/`Draining` path; it is never ignored or converted to zero.

**REFACTOR.** The supervisor remains the only settlement owner. Query streams,
telemetry, and test controls may observe its result but may not await metrics,
clean scratch, or publish a terminal independently.

### Scenario 3 — Existing topology and deployment proofs stay minimal

**Behavior.** Listener and deployment regression coverage proves the one
production shape without multiplying equivalent fixture profiles. Maps
REQ-009, AC-005, AC-008.

**RED.** Narrow
`peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete` so
its uniform Oracle topology proof runs only the exact three-Oracle/one-Scribe
shape used by Scenario 1; retain its listener, membership, mTLS, readiness, and
shutdown assertions. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::listener::peer_listener_is_isolated_mtls_and_role_complete)' --run-ignored=all"
```

**GREEN.** Replace `ORACLE_TOPOLOGIES` and its loop with one three-Oracle call.
Do not change deployment manifests: the existing mixed and role-separated
fixtures are profile-neutral semantic proof of private bind, mTLS identity,
target composition, readiness, and shutdown. `mise run
check:bifrost-oracle-deploy` remains their static proof.

**REFACTOR.** `Interactive` and `Analytical` remain execution paths, never pod
roles or deployment-profile labels.

## Cross-scenario decisions and authority

The representative SQL, exchange, spill, exact result, telemetry, coordinator
swap, listener isolation, and cleanup assertions live in one process journey so
an unrelated sort cannot satisfy Journey B. The fixed 512 MiB process snapshot
produces a 256 MiB query grant through production math; the fixed measured sort
input exceeds it without changing production limits. The identified output
`SortExec` metric set is the only spill evidence. The existing process-owned
spill directory is the only filesystem owner. Metric settlement is part of Task
2's graph lifecycle and uses its immutable deadline and terminal state machine.

Authority: `AGENTS.md`, the approved spec REQ-003/005/007/009/011 and Journey B,
`architecture/bifrost-design.md`, `architecture/wyrd-security-posture.md`,
`architecture/operations/runbooks.md`,
`architecture/references/domain/datafusion.md`, and
`architecture/references/domain/analytical-operations-reliability.md`.

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run check:bifrost-oracle-deploy
mise run check:bifrost-resource-governance
mise run check:object-store-pin
git diff --check
```

## Completion evidence

- One three-Oracle/one-Scribe process journey run twice with different
  coordinators, the asserted 256 MiB production-derived grant, 300,000 exact
  ordered result rows and digest, a measured result working set above that
  grant, asymmetric source rows, narrow pre-sort state, remote scan/lease
  activity, and positive
  exchange evidence.
- Positive native spill count/bytes/rows from the uniquely identified output
  `SortExec` in those same executions, complete result parity proving spill
  read, and process-owned spill directories returned to their pre-query
  entry/byte baselines.
- Deadline-gated metric settlement evidence for success and missing-metric
  failure, including retained `Draining`, false readiness, and joined shutdown.
- Bounded production metric deltas/labels and zero live ownership/current
  gauges after each successful terminal.
- Public/private listener probes for both coordinators plus the narrowed
  three-Oracle listener regression and unchanged profile-neutral deployment
  manifests.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if the baseline needs a dependency fork,
materialized shuffle, new planner/scheduler, broader operators, automatic
retry, synthetic physical evidence, or weakened tenant/terminal semantics.
Return `PLAN_BLOCKED` if the pinned DataFusion metrics cannot expose positive
spill count/bytes/rows for the same completed plan or Task 2's lifecycle cannot
own the rewritten-plan metric future without changing a public contract; report
the exact missing dependency capability rather than adding a parallel owner.
