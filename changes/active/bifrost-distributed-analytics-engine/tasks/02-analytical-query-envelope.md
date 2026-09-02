---
id: BIFROST-R4-T02-QUERY-ENVELOPE
title: Own one bounded one-attempt Analytical envelope through supervised settlement
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 4
depends_on: [BIFROST-R4-T01-R01-GRAPH-LEASE-REMEDIATION]
requirements: [REQ-002, REQ-004, REQ-005, REQ-006, REQ-007, REQ-009, REQ-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-006, INV-007, INV-008]
acceptance: [AC-001, AC-003, AC-004, AC-005, AC-007, AC-008]
parent_task: BIFROST-R3-T1-INACTIVE-CLOSEOUT
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# Analytical query envelope and supervised settlement

## Outcome and value

After supported physical planning makes Analytical selection irreversible, the
leader reserves the complete immutable follower cut once immediately before the
first distributed dispatch. The existing `AnalyticalSupervisor` graph consumes
the query's already-admitted envelope and retains one lifecycle task until every
channel, stage, task, cache entry, exchange, pool, scratch allocation, spill
file, and reservation has joined and released. There is no retry, second leader
admission, exchange sub-pool, or per-destination reservation state machine.
Failure cannot become partial success or consume the Interactive floor.

Required execution skill: `$wyrd-implement`.

## Current-state amendment

- **Retain:** `AnalyticalExecutionHandle`, `AnalyticalParticipantCut`, the bulk
  `reserve_destinations` operation with server-generated reservation IDs,
  `AnalyticalChannelResolver`, `AnalyticalSupervisor`, `OracleSpillRuntime`,
  `OracleAdmission`, the existing two-second follower pending TTL, and the
  pinned dependency adapters.
- **Delete:** `retry_pre_egress`, attempt-one state/telemetry/tests, predicted
  exchange precharge, `exchange_buffer_bytes`, `EXCHANGE_CONSUMER`, and
  exchange-only memory splitting. Delete from this proposed task the
  per-destination `Unreserved | Reserving | Reserved | Released` state machine,
  reservation tasks, deterministic reservation-ID derivation,
  `reserve_or_reuse` protocol changes, resolver-stage reservation wiring,
  graph-wide `JoinSet`, global settlement queue, and stream lifecycle enum.
- **Invalidate:** the current call to `try_acquire_query` inside
  `AnalyticalExecutionHandle::lease_session`, reservation before supported
  physical-plan validation, spawning or best-effort cleanup from `Drop`, and
  log-and-ignore settlement failures.
- **Unfinished:** move the existing admitted guard into the registered leader
  graph; reserve the frozen follower cut once at the final pre-dispatch boundary;
  retain one lifecycle task in that graph; make terminal, readiness, and shutdown
  wait on or report its result; and keep cleanup ambiguity visible as `Draining`
  until it is authoritatively settled.

## Owners, scope, consumers, and non-goals

Primary owners are `oracle/analytical.rs` (leader graph, immutable participant
cut, and bulk participant reservation), `oracle/analytical_supervisor.rs`
(graph-owned admission, descendants, lifecycle task, `Draining`, and
settlement), `oracle/admission.rs` and `resources.rs` (one envelope and floor),
`oracle/query_stream.rs` (terminal ordering and cancellation signal),
`oracle/spill.rs`, and the existing channel resolver. The current SQL attempt
orchestration in `oracle/mod.rs` must transfer, rather than duplicate, admission
ownership and must invoke reservation only after the physical plan is supported
and a real exchange makes Analytical selection final. Task 3 consumes the
finished inactive engine; Task 4 consumes the same handle for production
routing.

Do not add another runtime, registry, supervisor, scheduler, memory root,
scratch owner, retry loop, listener, public contract, protocol method, or
routing API. Do not derive reservation IDs on the leader. Physical operator
qualification belongs to Task 3; this task owns the pre-dispatch ordering seam
and its negative proof.

## Ordered implementation scenarios

### Scenario 1 — Reserve the immutable participant cut once before dispatch

**Behavior.** Planning, unsupported physical shapes, and no-exchange fallback
make zero reserve RPCs. After supported physical planning and irreversible
Analytical selection, reserve the complete immutable remote participant cut
once, immediately before the first stage dispatch; the dependency cannot issue
`SetPlan` or `ExecuteTask` until all reservations have succeeded. Each follower
receives exactly one reserve RPC carrying its server-generated reservation ID
and the existing signed graph/participant authority. Partial reservation failure
releases every accepted reservation. Reservation or release ambiguity fails the
query and keeps the graph supervisor-visible as `Draining` until release
acknowledgement or authoritative expiry of the existing pending TTL. Maps
REQ-004, REQ-005, REQ-007, INV-002, INV-003, INV-004, INV-005, AC-001, AC-004.

**RED.** Add
`oracle::analytical::tests::participant_cut_is_reserved_once_immediately_before_dispatch`.
Use the existing test transport with deterministic plan, reserve, dispatch, and
release barriers. Assert zero reserve calls for planning failure, unsupported
shape, and no-exchange fallback; for a selected plan assert one reserve per
remote participant, all reserve acknowledgements before the first dispatch,
unchanged server-generated IDs in the resolver's frozen destinations, and the
two-second pending TTL. Refuse participant N after earlier acceptances and
assert exact release of each accepted reservation, a failed terminal, and no
dispatch. Make one release acknowledgement ambiguous and assert `Draining`,
false readiness, and retention until acknowledgement or authoritative pending
expiry. It fails because reservation currently happens inside `lease_session`
before physical-plan support and exchange are known. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::participant_cut_is_reserved_once_immediately_before_dispatch)'
```

**GREEN.** Keep `reserve_destinations` as the sole bulk reservation operation
and `AnalyticalParticipantReservations` as its one accepted-reservation owner.
Remove reservation from session leasing. The SQL attempt orchestration first
builds and validates the distributed physical plan against the immutable cut;
unsupported or exchange-free plans take the existing pre-selection fallback
without a reservation. Once selection is final, signal the already-registered
graph lifecycle task to call `reserve_destinations` exactly once. That task
retains the accumulated `AnalyticalParticipantReservations` while the bulk loop
runs and publishes the completed destination map to the resolver only after all
followers accept; only then may dispatch start. Preserve follower-minted IDs,
the signed participant cut, and the existing pending TTL. On partial reserve
failure, the same task explicitly releases its accumulated acceptances before
reporting failure. Change the reservation owner's release operation to return
and retain unresolved releases instead of draining and logging them. Do not turn
an unknown release result into success: the task keeps the graph `Draining`
until the existing release RPC acknowledges or the follower's pending
reservation is authoritatively expired.

**REFACTOR.** Planning reads only immutable cut URLs and fences. Reservation
ownership stays graph-wide and bulk; the resolver substitutes already-issued
IDs and never reserves, derives an ID, or owns a destination state machine.

### Scenario 2 — One admitted guard, pool, runtime, and finite result transport

**Behavior.** `AnalyticalExecutionHandle::lease_session` consumes the existing
leader `AdmittedQueryGuard`; it never acquires another leader envelope. The
existing `AnalyticalSupervisor` graph owns that guard, its runtime, immutable
participant cut, reservations, cancellation tree, absolute deadline, and all
descendants until successful settlement. Each follower GraphLease owns exactly
one follower-local query envelope. Operators and exchanges on a participant use
that participant's same DataFusion pool; different queries use different pools.
Maps REQ-005, REQ-006, INV-005, INV-006, INV-007, AC-004, AC-008.

**RED.** Add
`oracle::analytical::tests::analytical_graph_reuses_one_query_pool_per_process`.
Assert the leader admission count remains one across leasing, registration,
execution, and cleanup; follower admission is one per graph; the supervisor's
graph and the leader session expose the admitted guard's original pool identity;
operator and exchange allocations hit that pool's same current/peak counter;
distinct queries use distinct pools; and Analytical never enters the Interactive
floor. Fill the finite result channel to prove the producer blocks/refuses
without unbounded allocation, then cancel and assert its lifecycle task joins.
Assert no exchange child/prediction field or second `try_acquire_query` remains.
Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::analytical_graph_reuses_one_query_pool_per_process)'
```

**GREEN.** Change `AnalyticalExecutionHandle::lease_session` and its caller in
`oracle/mod.rs` to transfer the existing `AdmittedQueryGuard` into the
`AnalyticalSupervisor` graph registration. Build the graph runtime from that
guard's pool and spill grant; remove the handle's `OracleResources`
`try_acquire_query` path. Return only the private session/lifecycle handle needed
by the SQL attempt and query stream; do not return or synthesize a second
admission owner. Install Task 1's follower envelope into its graph runtime.
Remove exchange child allocation and let the pinned exchange consumer allocate
from the installed pool. Preserve finite worker, graph, task/partition, result,
queue, scratch, cancellation, and deadline limits with checked arithmetic before
dispatch. Keep the existing bounded result sender inside the same graph; a full
or closed result channel signals its lifecycle task, and no detached producer
may outlive the receiver.

**REFACTOR.** The supervisor graph is the leader's sole envelope owner;
`OracleQueryResources` remains each participant-local pool owner and measurement
point. No wrapper may introduce another ceiling or admission.

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
failure to the graph lifecycle task, then emit one failed terminal only after
that task reports successful cleanup or retained `Draining` failure.

**REFACTOR.** Do not rename retry into recovery; a caller's later submission is
a new public query with new identity and admission.

### Scenario 4 — One supervisor-owned lifecycle task and retained cleanup failure

**Behavior.** Each registered leader graph retains one lifecycle task in the
existing `AnalyticalSupervisor`. Success, cancellation, deadline, caller drop,
peer loss, resource failure, and shutdown signal that task; terminal-producing
paths await its result. The task performs the single cleanup sequence, removes
the graph only after successful cleanup, and retains the graph as `Draining` on
timeout, release ambiguity, or cleanup failure. A failed cleanup prevents a
success terminal and removes readiness. Shutdown cancels and joins these same
graph tasks. Query-stream `Drop` only signals cancellation; it does not spawn,
abort, release, or reproduce cleanup. Maps REQ-007, REQ-009, REQ-011, INV-003,
INV-004, INV-005, AC-001, AC-004, AC-005, AC-007.

**RED.** Add
`oracle::analytical::tests::leader_lifecycle_task_joins_every_owner_and_retains_failure`.
Gate each descendant class deterministically and drive success, explicit
cancellation, deadline, raw stream drop, and shutdown. Assert the graph task
remains supervisor-owned after stream drop; slots, original admission, pool,
scratch, reservations, graph/attempt/task/cache/exchange ownership, and active
gauges remain until their gates join; and successful cleanup releases every
owner exactly once before graph removal and terminal success. Inject spill and
reservation cleanup failures and assert failed terminal, retained `Draining`,
false readiness, and shutdown residue. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::leader_lifecycle_task_joins_every_owner_and_retains_failure)'
```

**GREEN.** Store one lifecycle `JoinHandle` and its cancellation/completion
signals in the existing `AnalyticalGraphState`; do not add a second supervisor,
registry, graph-wide `JoinSet`, or global settlement worker. The task alone owns
the order: close dispatch; cancel on non-success; join coordinator channels,
stage drivers, returned streams, tasks, cache invalidation, and exchanges;
verify pool idle; clean and verify graph scratch/spill; release or authoritatively
expire follower reservations; release the admitted guard; then remove graph and
permit terminal success. On any error, keep the graph entry and lifecycle result
as `Draining`, fail readiness, and force a failed terminal. `OracleQueryStream`
terminal handling signals and awaits the lifecycle handle. Its `Drop` path only
cancels; supervisor ownership keeps the task alive and makes shutdown join it.

**REFACTOR.** The lifecycle task is the single settlement sequence. Query
stream, shutdown, and error sites may signal or await it but may not reproduce
its order, spawn substitute cleanup, or log-and-ignore its result.

## Cross-scenario decisions and authority

The immutable participant cut is selected before reservation; bulk reservation
occurs only after supported physical planning and immediately before the first
dispatch. The existing resolver consumes already-reserved destinations and is
not a reservation owner. One absolute deadline covers reserve, execute, cleanup,
and terminal settlement. The dependency's aggregate byte backpressure remains
the honest dependency boundary; Wyrd does not claim an item limit.

No new wire or public contract is required. The supervisor's existing graph map
is the authoritative lifecycle registry: success removes its entry; cleanup
ambiguity retains the same entry as `Draining`, which is included in readiness
and shutdown inspection. The one lifecycle task is per graph and stored there;
there is no global queue or scheduler.

Authority: `AGENTS.md`, `architecture/wyrd-design.md`,
`architecture/wyrd-doctrine.mdx`, `architecture/bifrost-design.md`,
`architecture/wyrd-security-posture.md`,
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

- Plan/reserve/dispatch traces proving unsupported and no-exchange paths make
  zero reserve RPCs and selected execution reserves every follower once before
  any dispatch.
- Partial-reservation and ambiguous-release evidence proving accepted leases
  are released and uncertain cleanup remains `Draining` until acknowledgement
  or authoritative pending-TTL expiry.
- Admission and pool identity snapshots proving one leader admission, one
  participant-local memory pool, finite result pressure, and preservation of
  the Interactive floor.
- Source and telemetry proof that no automatic retry, exchange child,
  per-destination reservation machinery, or second acquisition remains.
- Raw stream-drop and shutdown evidence proving the supervisor-owned lifecycle
  task remains joined and visible.
- Per-outcome snapshots proving cleanup failure retains `Draining`, removes
  readiness, prevents success, and successful cleanup releases every owner
  exactly once.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if correctness requires reservation before
supported physical-plan validation, automatic retry, a separate exchange pool,
a weakened Interactive floor, best-effort cleanup, or behavior outside the
approved obligations. Return `PLAN_BLOCKED` only if current repository or pinned
dependency APIs cannot separate supported physical planning from first dispatch,
cannot transfer the admitted guard into the existing supervisor graph, or cannot
surface authoritative pending-reservation expiry without a new protocol. Report
the exact missing capability; do not restore the deleted lazy-reservation design
or invent a new contract.
