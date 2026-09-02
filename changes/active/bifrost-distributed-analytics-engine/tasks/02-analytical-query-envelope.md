---
id: BIFROST-R3-T02-QUERY-ENVELOPE
title: Own one bounded one-attempt Analytical envelope through joined settlement
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 3
depends_on: [BIFROST-R3-T01-GRAPH-LEASE]
requirements: [REQ-004, REQ-005, REQ-006, REQ-007, REQ-009, REQ-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-001, AC-004, AC-005, AC-007, AC-008]
parent_task: BIFROST-R3-T1-INACTIVE-CLOSEOUT
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# Analytical query envelope and joined settlement

## Outcome and value

The Analytical leader reuses its already-admitted query envelope, lazily
reserves each follower immediately before that follower's first stage dispatch,
and owns one attempt until every channel, stage, task, cache entry, exchange,
pool, scratch allocation, spill file, and reservation has joined and released.
There is no retry or exchange sub-pool. Failure cannot become partial success or
consume the Interactive floor.

Required execution skill: `$wyrd-implement`.

## Current-state amendment

- Retain `AnalyticalExecutionHandle`, `AnalyticalParticipantCut`,
  `AnalyticalChannelResolver`, `AnalyticalSupervisor`, `OracleSpillRuntime`,
  `OracleAdmission`, and the pinned dependency adapters.
- Replace eager participant reservation in `lease_session` with first-channel
  lazy reservation at the existing async `ChannelResolver` boundary.
- Delete `retry_pre_egress`, attempt-one state/telemetry/tests, predicted
  exchange precharge, `exchange_buffer_bytes`, `EXCHANGE_CONSUMER`, and
  exchange-only memory splitting.
- Replace spawning `Drop` cleanup and logged settlement errors with explicit
  joined settlement whose failure remains supervisor/readiness-visible.

## Owners, scope, consumers, and non-goals

Primary owners are `oracle/analytical.rs` (leader graph and participant
reservations), `oracle/analytical_supervisor.rs` (descendant ownership and
settlement), `oracle/admission.rs` and `resources.rs` (one envelope and floor),
`oracle/query_stream.rs` (terminal ordering), `oracle/spill.rs`, and the current
channel resolver. Task 3 consumes the finished inactive engine; Task 4 consumes
the same handle for production routing.

Do not add another runtime, scheduler, memory root, scratch owner, retry loop,
listener, or routing API. Physical operator qualification belongs to Task 3.

## Ordered implementation scenarios

### Scenario 1 — Reservation immediately before dispatch

**Behavior.** Unsupported/no-exchange preparation reserves no follower; the
first channel resolution for a selected destination reserves it once; concurrent
resolutions share the in-flight result; partial fan-out releases every accepted
reservation. Maps REQ-004, REQ-005, INV-002, INV-004, INV-005, AC-001, AC-004.

**RED.** Add
`oracle::analytical::tests::participant_reservation_is_lazy_shared_and_fully_released`.
Use deterministic resolver barriers to assert zero reservations during planning,
one reserve RPC for concurrent first resolution, unchanged two-second TTL from
acceptance, and exact release after partial fan-out failure. It fails because
`lease_session` currently reserves all participants. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::participant_reservation_is_lazy_shared_and_fully_released)'
```

**GREEN.** Make `AnalyticalParticipantReservations` own a destination-keyed
`Unreserved | Reserving(shared completion) | Reserved(id, connection child) |
Released` state. `AnalyticalChannelResolver::get_worker_client_for_url` starts
the reserve RPC immediately before the dependency sends its first stage to that
URL. Equivalent callers await one result. Settlement closes new resolutions,
joins channel children, and explicitly releases each accepted reservation once.

**REFACTOR.** The resolver delegates lifecycle to the reservation owner; it does
not grow a second map or read live membership outside the immutable cut.

### Scenario 2 — One pool and runtime per participant process

**Behavior.** The leader reuses its existing `AdmittedQueryGuard`; each follower
GraphLease owns exactly one local query envelope; operators and exchanges use
that participant's same DataFusion pool. Different queries use different pools.
Maps REQ-005, REQ-006, INV-005, INV-006, INV-007, AC-004, AC-008.

**RED.** Add
`oracle::analytical::tests::analytical_graph_reuses_one_query_pool_per_process`.
Assert leader admission count remains one, follower count is one per graph,
operator and exchange allocations hit the same pool identity/current/peak
counter, distinct queries have distinct pools, and floor capacity never enters
an Analytical grant. Assert no exchange child/prediction field remains. Exact:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::analytical_graph_reuses_one_query_pool_per_process)'
```

**GREEN.** Pass the admitted guard's runtime inputs, pool, scratch allocation,
cancellation token, and deadline into `AnalyticalExecutionHandle`; do not call
`try_acquire_query` again on the leader. Install Task 1's follower envelope into
its graph runtime. Remove exchange child allocation and let the pinned exchange
consumer allocate from the installed pool. Preserve finite worker, graph,
task/partition, result, queue, scratch, and deadline limits with checked
arithmetic before dispatch.

**REFACTOR.** `OracleQueryResources` remains the sole participant-local pool
owner and measurement point; no wrapper may introduce another ceiling.

### Scenario 3 — One attempt and terminal failure

**Behavior.** Peer, transport, protocol, resource, deadline, cancellation, or
execution failure after selection starts no successor attempt and cannot fall
back or validate preceding rows. Maps REQ-007, INV-003, INV-008, AC-003, AC-004,
AC-008.

**RED.** Add `oracle::analytical::tests::peer_loss_is_one_terminal_attempt`.
Inject peer loss before and after a data frame; assert attempt count is one,
the failure terminal invalidates all prior frames, and no retry API/counter/
state remains. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::peer_loss_is_one_terminal_attempt)'
```

**GREEN.** Delete `retry_pre_egress` and all attempt-one branches. Collapse the
supervisor attempt identity to the single graph attempt while retaining typed
attempt zero where the dependency wire requires it. Route every post-selection
failure into cancellation plus joined settlement, then one failed terminal.

**REFACTOR.** Do not rename retry into recovery; a caller's later submission is
a new public query with new identity and admission.

### Scenario 4 — Joined leader settlement and retained cleanup failure

**Behavior.** Success, caller drop, cancellation, deadline, peer loss, resource
failure, and shutdown join all descendants before release. Cleanup timeout or
error never emits success, remains draining, removes readiness, and is reported
by shutdown. Maps REQ-007, REQ-009, REQ-011, INV-003, INV-004, INV-005, AC-001,
AC-004, AC-005, AC-007.

**RED.** Add
`oracle::analytical::tests::leader_settlement_joins_every_owner_and_retains_failure`.
Gate each descendant class deterministically and assert that slots, pool,
scratch, reservations, graph/attempt/task/cache/exchange ownership, and active
gauges remain until its gate joins. Inject spill cleanup timeout and assert
`Draining`, false readiness, failed terminal, and shutdown residue. Exact:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::leader_settlement_joins_every_owner_and_retains_failure)'
```

**GREEN.** Give the leader graph one explicit async `settle(outcome)` order:
close dispatch; cancel on non-success; join coordinator channels/stage drivers/
returned streams/tasks/cache invalidation; verify exchange and pool idle; clean
and verify graph scratch; settle follower reservations; settle supervisor state;
then release admission and permit terminal emission. `Drop` signals cancellation
and leak telemetry only. Keep failed cleanup in the supervisor's draining map.

**REFACTOR.** `query_stream.rs` awaits one owner method; it must not recreate the
settlement sequence or log-and-ignore errors.

## Cross-scenario decisions and authority

The dependency's async `ChannelResolver` is the selected lazy-reservation seam;
no fork is needed. The dependency's existing aggregate byte backpressure is
accepted honestly; Wyrd does not claim an item limit. One absolute deadline
covers reserve, execute, cleanup, and terminal settlement.

Authority: `architecture/bifrost-design.md`,
`architecture/references/domain/datafusion.md`,
`architecture/references/domain/analytical-operations-reliability.md`, and
`architecture/operations/reliability-and-recovery.md`.

## Broader verification

```bash
mise run fmt
mise run lints
mise run test:bifrost
mise run test:bifrost:journey:oracle
mise run check:bifrost-resource-governance
mise run check:bifrost-oracle-deploy
git diff --check
```

## Completion evidence

- Resolver event trace proving reserve timing, one concurrent RPC, and release.
- Pool/runtime identity and Interactive-floor snapshots.
- Source and telemetry proof that no automatic retry or exchange child remains.
- Per-outcome joined owner snapshots plus readiness/shutdown residue on failure.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if correctness requires eager reservation,
automatic retry, a separate exchange pool, a weakened Interactive floor, or
best-effort cleanup. Return `PLAN_BLOCKED` only if the pinned resolver API cannot
intercept the first dispatch; current source proves it can.
