---
task_id: BIFROST-R3-T1-INACTIVE-CLOSEOUT
title: Close the inactive Analytical engine under one exact joined graph lease
kind: remediate
status: proposed
approved_spec: SPEC-bifrost-distributed-analytics-engine
approved_revision: 3
parent_task: BIFROST-T1-UNIFIED-PEER-REMEDIATION
frozen_candidate: f1ac4cb01
reviewed_range: eae68e5ca3080b4e58624eb394fba6988afabe02..f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
uncommitted_implementation_delta: none
dependencies: []
remediates:
  - FIND-BIFROST-R3-T1-INACTIVE-CLOSEOUT-1
  - FIND-BIFROST-R3-T1-INACTIVE-CLOSEOUT-2
  - FIND-BIFROST-R3-T1-INACTIVE-CLOSEOUT-3
  - FIND-BIFROST-R3-T1-INACTIVE-CLOSEOUT-4
  - FIND-BIFROST-R3-T1-INACTIVE-CLOSEOUT-5
  - FIND-BIFROST-R3-T1-INACTIVE-CLOSEOUT-6
mapped_requirements: [REQ-003, REQ-004, REQ-005, REQ-007, REQ-009, REQ-011]
mapped_invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-007, INV-008]
mapped_acceptance: [AC-001, AC-002, AC-004, AC-005, AC-007, AC-008]
---

# Superseded task — Revision 3 inactive Analytical closeout

## Outcome

Finish only the inactive-engine work still required by approved revision 3.
After this task, the existing private peer plane can run one supported
cross-process Analytical graph under one exact process-local envelope per
participant, one immutable authority, one attempt, and one joined cleanup
path. Production raw-SQL routing remains Interactive-only until Task 2.

This successor does not re-open the already landed peer-plane design and does
not complete the superseded remediation task by accumulation. It removes
revision-3-invalidated retry and exchange-subpool behavior while closing the
remaining GraphLease, physical-execution, cleanup, telemetry, and deployment
gaps.

This is the single cohesive remediation for the six validated findings from
the immutable reviewed range above. They share one lifecycle boundary: the
query is admitted, its graph activates and dispatches, and its owners settle.
Splitting that boundary would require temporary ownership contracts that
revision 3 does not need.

Required execution skill: `$wyrd-implement`.

## Reviewed-candidate amendment

### Candidate identity and verification

- The committed implementation candidate is `f1ac4cb01` on
  `oracle-distributed`. No uncommitted production-code delta exists; current
  uncommitted files are architecture/skill/spec/task authority.
- The source remediation is
  `remediation/01-t1-inactive-distributed-execution-review-remediation.md`.
  The claimed implementation record is
  `remediation/remediation-slice-ledger.md`.
- A current `mise run test:bifrost:journey:oracle` run executed 15 tests: 14
  passed and
  `analytical_inactive::pg_inactive_analytical_raw_sql_executes_join_and_partial_final_aggregate_on_followers`
  failed. The ledger's Slice 5 warning therefore remains current.
- `GraphLeaseRequest` currently proves only reservation ID, graph, and query
  ID. `ReservationRegistry::lease_graph` destructively removes the pending
  entry before fallible runtime/supervisor registration. The active lease and
  graph guard are published in separate owners.
- `AnalyticalParticipantReservations::Drop` and
  `AnalyticalConnectionLease::Drop` spawn best-effort cleanup. Cleanup failure
  is logged, and the participant release path treats peer release failure as a
  successful terminal concern. This does not satisfy joined cleanup.
- `AnalyticalExecutionHandle::lease_session` reserves every remote participant
  before local graph admission and distributed physical dispatch, so the
  two-second pending TTL begins too early.
- The candidate still owns an exchange child budget and automatic pre-egress
  retry. Revision 3 requires one query-local DataFusion pool per participant
  process and no automatic retry.
- Independent review returned `REMEDIATE` with the six finding IDs recorded in
  frontmatter. Findings 1–5 confirm the ownership defects above. Finding 6
  confirms that the three replacement tests named below do not exist yet and
  that the ledger's 14/15 Oracle journey result and open Slices 6–8 are not
  completion evidence.

### Validated-finding closure

- Finding 1 closes only when Scenario 1 and the named activation/rollback test
  prove complete-authority comparison, publish-after-registration, shared
  equivalent activation, and rollback or release on every injected failure.
- Finding 2 closes only when Scenario 2 proves unsupported preparation reserves
  nobody, concurrent first resolution reserves once, and partial fan-out
  releases every accepted reservation.
- Finding 3 closes only when Scenario 3 proves the leader reuses the envelope
  already admitted through the same path as Interactive and no participant has
  a second pool, predicted exchange charge, or exchange sublimit.
- Finding 4 closes only when Scenario 5 proves explicit joined settlement and
  makes a cleanup timeout or error remain draining and readiness-visible.
- Finding 5 closes only when Scenario 4 proves peer loss starts no successor
  attempt and no retry API, counter, state, test, or retention remains.
- Finding 6 closes only when the two named journeys prove the physical/spill,
  joined-cleanup, production-telemetry, and private-deployment obligations with
  zero residual ownership. The old ledger is historical evidence, not closure.

### Retained implementation

- Retain Slices 1–4: the server-owned private listener, mandatory mTLS,
  pre-body workload authentication, independent peer-ticket keyring, exact
  purpose tickets, role-neutral authenticated transport, immutable participant
  cut, stable Scribe identity, readiness/shutdown integration, and their
  focused tests.
- Retain the existing `ReservationRegistry`, `OracleResources`,
  `OracleSpillRuntime`, `AnalyticalSupervisor`, process-cluster harness, typed
  graph identities, real follower reservation IDs, and the basic
  one-activation-per-follower success evidence from Slice 5.
- Retain the pinned DataFusion 55 / Arrow-Parquet 59.2 /
  `datafusion-distributed` universe and both required private wire adapters.

### Invalidated obligations and code

- Delete automatic attempt-one retry, retry-only APIs, retry telemetry, retry
  tests, and prose. A peer or transport loss after Analytical selection is one
  terminal failed attempt; a caller may submit a new logical query.
- Delete predicted exchange precharge and operator/exchange sublimits. Operators
  and exchanges allocate from the same query-local DataFusion pool. Preserve
  only configured dependency byte backpressure and honest exchange telemetry.
- Do not prove one-, two-, three-, and six-Oracle matrices. Use the smallest
  local topology for local behavior and the smallest real cross-process
  topology that exercises the required graph.
- Do not add public EXPLAIN, windows, correlated subqueries, deduplicating sets,
  UDAFs, a new scheduler, registry, resource framework, listener, or test-only
  telemetry owner.

### Unfinished obligations

- Complete authority validation and rollback-safe exactly-once GraphLease
  activation under either `SetPlan`/`ExecuteTask` ordering and duplicates.
- Reserve a selected follower immediately before its first stage dispatch,
  retain the two-second pending TTL, and return every unused reservation.
- Make each participant's graph lease retain its exact process-local envelope,
  runtime, query-local pool, scratch/spill, cancellation tree, deadline,
  authority, and all descendants until joined.
- Remove best-effort/detached cleanup and retain failed cleanup for readiness
  and shutdown evidence.
- Prove the revision-3 physical baseline and private deployment isolation with
  production telemetry and zero residual ownership.
- Reconcile the retry, EXPLAIN, exhaustive-operator, and exchange-child prose in
  `architecture/bifrost-design.md` and the focused Bifrost references to the
  approved revision before claiming task completion.

## Owners, scope, and consumers

Primary owners:

- `crates/vala/vala-bifrost-redux/src/oracle/dispatcher.rs` — pending
  reservation ownership and transfer guard.
- `crates/vala/vala-bifrost-redux/src/oracle/analytical.rs` —
  `AnalyticalExecutionHandle`, per-graph reservation set, follower activation,
  graph settlement, and runtime installation.
- `crates/vala/vala-bifrost-redux/src/oracle/analytical_supervisor.rs` — joined
  graph/attempt/task/cache ownership and retained cleanup failure.
- `crates/vala/vala-bifrost-redux/src/oracle/analytical_transport.rs` and
  `oracle/peer.rs` — exact graph-authority projection on reserve and stage
  tickets; no new transport owner.
- `crates/wyrd-spec/src/vala/api.rs` and the existing proto source only for the
  private reservation fields required to carry exact graph authority.
- `crates/wyrd/wyrd-testing/src/bifrost/process_cluster*` and existing Oracle
  and server journey targets for real process evidence.
- Existing deployment manifests/checks and the named architecture authorities.

Task 2 consumes the settled `AnalyticalExecutionHandle` and must not rebuild
reservation, runtime, cancellation, or cleanup ownership. No Python,
TypeScript, MCP, public route, or public request work belongs here.

## Design closure

### 1. Query envelope and authority

Follow the existing Interactive resource path without adding an Analytical
resource model. `OracleAdmission::admit` creates one `AdmittedQueryGuard` whose
`LocalPermit` contains one `OracleQueryResources`; that resource owner creates
one query-local DataFusion pool and charges it to the aggregate Oracle
governor. Different queries never share this pool.

`Oracle::lease_analytical_session` passes the already-admitted guard's pool,
spill limit, granted-memory ceiling, target partitions, cancellation token,
and deadline into `AnalyticalExecutionHandle`. The handle must not call
`OracleResources::try_acquire_query` for the leader. Build the leader runtime
with the same `OracleSpillRuntime::build_query_runtime` and
`OracleSessionShape::for_grant` path used by Interactive, then retain the graph
ownership inside that same `AdmittedQueryGuard` until joined settlement.

Each follower necessarily admits its own process-local envelope for the same
immutable graph authority. It creates exactly one query-local DataFusion pool
for that graph on that process. No process creates a second pool for the graph,
and no pool is shared between different queries. Operators and exchange on a
participant use that participant's one pool. The only shared resource object is
the existing aggregate process governor.

Introduce one Rust-native `GraphAuthority` value owned by the Analytical graph.
Its canonical digest binds the authenticated data tenant, public and
DataFusion query IDs, leader node/fence, destination node/fence, immutable
participant-cut digest, snapshot digest, permission digest, absolute deadline,
and graph identity. The reserve request stores those exact values; every stage
ticket carries the same authority digest plus its own operation/body/stage/task
binding. `AuthorizedQueryContext` has no Wyrd space field, so this private
authority does not add one or create a second namespace contract.

Place `GraphAuthority` and `graph_authority_digest` beside the existing stage
and reservation authority types in `oracle/peer.rs`. Hash the versioned,
length-prefixed canonical field sequence and the already sorted participant cut
with SHA-256 under the domain separator
`wyrd.oracle.graph.authority.v1\0`, returning the same lowercase 64-hex shape
used by current body and assignment digests. Reserve mint/verify and stage
mint/verify call this single helper; do not maintain parallel field lists.

The follower validates transport identity, purpose ticket, destination/fence,
full graph authority, reservation identity, pending expiry, and body digest
before plan decode, cache lookup, provider construction, or source IO. A
middle-stage coordinator may present its own authenticated source identity, but
it cannot change the original leader or authority stored by the reservation.

### 2. Reservation timing and leader ownership

Add one graph-scoped `AnalyticalParticipantReservations` owner with a closed
per-destination state: `Unreserved`, `Reserving`, `Reserved`, `Released`.
`AnalyticalChannelResolver` asks this owner to reserve the exact destination on
the first channel resolution immediately before that destination's first stage
dispatch. Concurrent channel resolutions await the same in-flight reservation
and reuse its follower-issued ID; they never reserve twice. Planning and
unsupported-shape validation happen before this transition.

The pinned dependency supports this decision directly: `ChannelResolver` uses
an async `get_worker_client_for_url(&Url)` call for every worker RPC. Extend the
existing `AnalyticalChannelResolver` implementation at that boundary; do not
patch or fork `datafusion-distributed`.

The owner retains every reservation and channel child until explicit async
settlement. On setup failure, cancellation, deadline, peer loss, success, or
shutdown it stops dispatch, joins channel work, then releases each unused or
active participant exactly once. `Drop` may signal cancellation and mark the
owner leaked, but must not spawn cleanup or report release.

### 3. Follower activation and publication

`AnalyticalStageIngress` is the activation coordinator. Replace its bare graph
map with a closed state per graph:

```text
absent -> activating(authority, shared completion) -> active(GraphLease)
                                      \-> absent on rollback/release
active -> draining -> released
```

The first valid stage message installs `activating` under the ingress lock.
Equivalent concurrent messages await that shared completion; mismatched
messages fail immediately. `ReservationRegistry` returns a private
`PendingGraphActivation` guard only after the complete pending tuple validates.
The guard owns the reservation permit and query resources while
`AnalyticalStageIngress` builds the query runtime and registers the graph with
`AnalyticalSupervisor`. Only after all fallible work succeeds does the ingress
publish one `GraphLease` containing the activation guard and supervisor graph
guard. An activation error rolls the guard back to the same pending entry when
its two-second TTL remains valid; otherwise it releases the exact resources.
All waiters receive the same result.

Remove `GraphLease::take_resources()`: an active lease must never survive as an
empty shell after its resources move elsewhere.

### 4. Memory, spill, and bounded execution

Use the Interactive construction unchanged: one admitted query envelope, one
query-local DataFusion pool, and one runtime per participant process. Install
that participant's exact admitted runtime and pool for all graph work on that
process. Operators and streamed exchange consumers draw dynamically from the
same pool; different queries remain isolated by their different pools beneath
the aggregate governor.

Delete `AnalyticalAttemptGrant.exchange_buffer_bytes`,
`EXCHANGE_CONSUMER`, `try_split_memory` for exchange, the predicted exchange
precharge, and every release field/counter/test that exists only for that
child. Do not replace them with a differently named estimate or sublimit.
Preserve only the pinned dependency's configured aggregate byte backpressure
without claiming an item bound or predicted allocation.

Keep finite configured limits for selected workers, Wyrd-owned admission
queues, active graphs, stage tasks/partition ranges, result transport, scratch,
deadline, and cancellation. Reject checked overflow before dispatch. Spill is
qualified only when DataFusion's own operator metrics and the graph scratch
owner prove write, read, and cleanup.

### 5. One attempt and joined settlement

Only attempt zero is valid in v1. Remove `retry_pre_egress`, attempt-one
admission, retry counters, and retry-specific retention. Peer loss is a failed
terminal and follows the same settlement path as cancellation, deadline,
resource exhaustion, protocol failure, caller drop, and shutdown.

One explicit async `GraphLease::settle(outcome)` performs this order:

1. close new stage/channel admission and signal the graph cancellation tree on
   failure;
2. join coordinator channels, stage drivers, returned streams, tasks, and
   cache invalidation;
3. verify exchange ownership and the DataFusion pool are idle;
4. clean and verify the graph scratch/spill allocation;
5. release supervisor graph state and participant reservations;
6. release the query envelope and publish the terminal cleanup outcome.

Timeout, poison, or cleanup failure retains the lease in `draining`, records a
bounded production failure, makes Analytical readiness false, and appears in
shutdown inspection. It must not emit a successful terminal or decrement the
active graph as though release completed.

### 6. Production evidence without a second telemetry owner

Keep `OracleTelemetry`, `AnalyticalAttemptTelemetry`, the resource governor,
and `GraphLease` settlement as the only producers. Do not add a registry,
exporter, polling task, or test-only production hook.

- Reuse the existing class-bounded query path, admission/refusal, active-query,
  cancellation, terminal-duration, spill, aggregate Oracle-memory, attempt,
  exchange-byte, and exchange-batch families.
- Remove `AnalyticalAttemptOutcome::Retried` and its registered series.
- Add only the missing closed production observations:
  `oracle_query_memory_peak_bytes{class}`, measured by the existing
  `OracleQueryResources` pool owner; one unlabeled
  `bifrost_oracle_analytical_worker_fanout` histogram finalized from the
  immutable participant set actually addressed; and
  `bifrost_oracle_analytical_cleanup_total{outcome}` plus
  `bifrost_oracle_analytical_graphs_draining`, where `outcome` is the closed set
  `success|timeout|failed`.
- Record query-pool current bytes from the pool's real `reserved()` value and
  its peak from reservation callbacks; do not estimate either from the plan or
  exchange configuration. The observation wrapper belongs inside
  `OracleQueryResources` and delegates to its existing pool—it is not another
  pool or resource ceiling.
- Aggregate DataFusion's real operator metrics at terminal settlement for
  spill count/bytes/read and result-equivalence evidence. Identity values stay
  in the existing scrubbed graph span and never become metric labels.

The named physical journey installs the normal production recorder and asserts
these real series changed during execution, then returned active/current and
draining gauges to zero. Descriptors registered at zero, configuration, plan
text, and test-only inspection structs are not sufficient evidence.

### 7. Evidence topology

Use one Oracle plus one Scribe only for local configuration/listener regression
where needed. Use three Oracle child processes plus one Scribe child for the
held-out inactive physical journey because the current planner needs two remote
followers to create the required real distributed stage. Do not repeat this at
six replicas.

The physical journey proves filtered/projected remote scans, multi-input
equi-join, fixed-width grouped `COUNT` and `SUM`, positive streamed exchange,
and a `SortExec` forced across its admitted memory threshold so DataFusion's
own spill metrics prove write/read/cleanup. Focused plan tests cover supported
`MIN` and `MAX`. The journey also proves exact result equivalence and zero
graph/attempt/task/cache/exchange/spill ownership after the terminal.

## Rejected alternatives

- A second reservation registry or graph supervisor: duplicates current owners.
- Reserving all cut participants during session construction: starts the
  two-second TTL before dispatch and strands unused reservations.
- Holding registry locks while building a runtime or registering DataFusion:
  blocks unrelated reservations and makes cancellation unsafe.
- Detached cleanup from `Drop`: cannot prove release or surface failure.
- Separate exchange memory pools or predicted precharge: prohibited by
  revision 3 and the reconciled architecture.
- Retaining automatic retry as harmless private behavior: contradicts the
  one-attempt public semantics and complicates graph ownership.

## Ordered TDD scenarios

Execute each scenario through RED, GREEN, REFACTOR before starting the next.

1. **Exact activation and rollback.** Either stage-message ordering activates
   once; identical concurrency waits/reuses; every authority mutation fails
   before IO; injected runtime/supervisor failure restores or releases the
   pending reservation without publishing a graph.
2. **Reservation timing.** Planning and no-exchange/unsupported preparation
   take no follower reservation; the first dispatch reserves once; concurrent
   channels reuse it; unused participants and partial fan-out failure release
   all accepted reservations.
3. **One query-local pool per participant.** The leader reuses its existing
   `AdmittedQueryGuard`; each follower admits exactly one envelope for the same
   graph; different queries have distinct pools; operator and exchange
   reservations are observed against the participant's one pool; no second
   leader admission, exchange prediction, or exchange child exists; finite
   worker/task/partition bounds refuse cleanly.
4. **One-attempt terminal failure.** An injected peer loss never starts attempt
   one and produces a failed terminal only after joined cleanup.
5. **Joined cleanup.** Success, cancellation, deadline, caller drop, peer loss,
   activation failure, resource exhaustion, and shutdown join every child;
   injected cleanup timeout remains visible and removes readiness.
6. **Physical baseline.** The real process journey proves the supported
   operators, exchange, qualified spill, exact result, bounded production
   telemetry, and zero retained ownership.
7. **Private topology closeout.** Peer services remain private, role targets and
   deployment manifests remain reachable and isolated, and production routing
   remains Interactive-only.

RED must fail on the missing behavior, not on a placeholder assertion. GREEN
implements only the current scenario. REFACTOR consolidates ownership on the
named structs and preserves the exact RED mutation.

## Named tests and exact focused commands

Pure owner test:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib \
  --features "$WYRD_REDUX_TEST_FEATURES" \
  -E 'test(=oracle::analytical::tests::graph_lease_activation_is_exact_order_independent_and_rollback_safe)'
```

Real GraphLease/physical/cleanup journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::analytical::inactive_analytical_graph_is_exact_physical_spilling_and_fully_joined)'"
```

Private server/deployment isolation journey:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test server -P journey -E 'test(=grpc_mount::production_query_stays_interactive_and_peer_plane_is_private)'"
```

## Broader verification

Run sequentially:

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run test:bifrost:journey:server
mise run check:bifrost-resource-governance
mise run check:bifrost-oracle-deploy
mise run check:proto-drift
mise run codegen:check
git diff --check
```

Run `mise run docs:check` if the selected architecture files are under the docs
site input; otherwise their Markdown/source diff and `git diff --check` are the
required documentation proof. Do not run `mise run gate` for this task slice.

## Completion evidence

- Current-state amendment reconciled against the final commit.
- Scenario-by-scenario RED/GREEN/mutation record.
- Authority-field matrix and activation state-transition evidence.
- Proof that no automatic retry or exchange subpool remains.
- Exact child PID/address/fence evidence for remote work.
- DataFusion operator, exchange, spill write/read/cleanup, result-equivalence,
  and zero-owner snapshots.
- Cleanup-failure readiness/shutdown evidence.
- Source-generated contract provenance and all command results.
- Final diff audit confirming retained Slices 1–4 were not redesigned.

## Stop conditions

Stop with `SPEC_REVISION_REQUIRED` if correctness requires retry, public
EXPLAIN, a second listener/registry/scheduler, a new dependency or fork, a
public path selector, a different terminal meaning, or broader operator
support.

## Authority

- `AGENTS.md`
- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`
- `architecture/bifrost-design.md`
- `architecture/wyrd-security-posture.md`
- `architecture/references/domain/olap-serving.md`
- `architecture/references/domain/datafusion.md`
- `architecture/references/domain/analytical-operations-reliability.md`
- `architecture/references/languages/implementation-execution.md`
- `architecture/references/languages/testing-workflows.md`
