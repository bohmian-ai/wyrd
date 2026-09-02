---
id: BIFROST-R4-T02-QUERY-ENVELOPE
title: Own one bounded one-attempt Analytical envelope through supervised settlement
kind: implementation
mode: RECONCILE
status: proposed
spec: SPEC-bifrost-distributed-analytics-engine
spec_revision: 4
depends_on: [BIFROST-R4-T01-R01-GRAPH-LEASE-REMEDIATION]
requirements: [REQ-002, REQ-003, REQ-004, REQ-005, REQ-007, REQ-009, REQ-011]
invariants: [INV-001, INV-002, INV-003, INV-004, INV-005, INV-007, INV-008]
acceptance: [AC-001, AC-002, AC-003, AC-004, AC-005, AC-007, AC-008]
parent_task: BIFROST-R3-T1-INACTIVE-CLOSEOUT
frozen_candidate: f1ac4cb01fe9ddda0a133cb58c955bab1e1cf7df
---

# Analytical query envelope and supervised settlement

## Outcome and value

Oracle first closes the supported physical-plan predicate, then reserves the
complete immutable follower cut once immediately before the first distributed
dispatch. The existing `AnalyticalSupervisor` graph consumes the query's
already-admitted envelope and retains one lifecycle task until every channel,
stage, task, cache entry, exchange, pool, scratch allocation, spill file, and
reservation has joined and released. There is no retry, second leader
admission, exchange sub-pool, or per-destination reservation state machine.
Failure cannot become partial success.

Required execution skill: `$wyrd-implement`.

## Current-state amendment

- **Retain:** `AnalyticalExecutionHandle`, `AnalyticalParticipantCut`, the bulk
  `reserve_destinations` operation with server-generated reservation IDs,
  `AnalyticalChannelResolver`, `AnalyticalSupervisor`, `OracleSpillRuntime`, the
  existing admitted guard from `OracleAdmission`, the existing two-second
  follower pending TTL, the synchronous `splitter::validate_supported`
  recursion, and the pinned dependency adapters.
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
- **Unfinished:** close the supported-plan predicate before reservation; move
  the existing admitted guard into the registered leader graph; plan from the
  immutable Oracle cut; publish the reservation-bearing participant cut once
  after complete reservation; retain one lifecycle task in that graph; make
  orchestration, terminal, readiness, and shutdown await or report its result;
  and keep cleanup ambiguity visible as `Draining` until it is authoritatively
  settled.

## Owners, scope, consumers, and non-goals

Primary owners are `oracle/splitter.rs` (the pure closed supported-plan
predicate), `oracle/analytical.rs` (leader graph, immutable participant cut, and
bulk participant reservation), `oracle/analytical_supervisor.rs` (graph-owned
admission, descendants, lifecycle task, `Active | Draining`, and settlement),
`resources.rs` (the transferred existing envelope), `oracle/query_stream.rs`
(terminal ordering and cancellation signal), `oracle/spill.rs`, and the
existing channel resolver. `wyrd-testing`'s existing Oracle process-cluster
owner supplies the real peer-loss and cancellation journey. The current SQL
attempt orchestration in `oracle/mod.rs` must transfer, rather than duplicate,
admission ownership and must invoke reservation only after the physical plan is
supported and a real exchange makes Analytical selection final. Task 3 consumes
the predicate and finished inactive engine for physical qualification; Task 4
consumes the same predicate and handle for production routing.

Do not add another runtime, registry, supervisor, scheduler, memory root,
scratch owner, retry loop, listener, public contract, protocol method, or
routing API. Do not derive reservation IDs on the leader. Physical operator
execution qualification belongs to Task 3; this task owns the closed predicate,
pre-dispatch ordering seam, and their negative proof. Aggregate admission and
the Interactive floor belong to Task 4.

## Ordered implementation scenarios

### Scenario 1 — Close the supported physical-plan predicate before reservation

**Behavior.** The accepted physical matrix is closed and direct:

- scan chains may contain `DataSourceExec`, `RemoteSourcePlaceholderExec`,
  `MemorySourceConfig`, `EmptyExec`, `FilterExec`, `ProjectionExec`, and the
  existing `TenantTripwireExec`;
- `AggregateExec` may use only `Partial`, `PartialReduce`, or
  `FinalPartitioned`. Every aggregate is a pinned built-in, non-distinct,
  order-insensitive expression with no aggregate `FILTER`: `COUNT(*)` or
  `COUNT(Int64)` has one non-null `Int64` accumulator state and a non-null
  `Int64` result; `SUM(Int64)`, `MIN(Int64)`, and `MAX(Int64)` each have one
  nullable `Int64` accumulator state and a nullable `Int64` result. Physical
  coercion must already have produced those exact argument types;
- distributed fan-out/fan-in may use only `RepartitionExec`,
  `CoalescePartitionsExec`, and `SortPreservingMergeExec`;
- `UnionExec` is accepted only as the direct source child of one
  `TenantTripwireExec`, with at least one authenticated source id, every source
  id present in `source_groups`, and all ids mapped to the same canonical table.
  A SQL/user set operation is never an accepted structural union, even when its
  inputs read the same table;
- joins may use only `HashJoinExec` with `JoinType::Inner`, at least one
  column-to-column equi-key, and `filter().is_none()`; and
- ordering/spill may use only `SortExec` with `fetch() == None`.

Every other aggregate mode, function, join form, semantic node, window, unknown
leaf, or plan without a real network exchange fails synchronously before IO or
reservation. Maps REQ-003, REQ-004, INV-007, INV-008, AC-002, AC-008.

**RED.** Add
`oracle::exec::tests::supported_analytical_plan_accepts_only_the_v1_baseline`.
Construct positive and one-semantic-mutation negative physical trees for every
matrix boundary. Exercise each accepted aggregate and assert its exact argument,
state-field, result-field, and nullability tuple. Reject `COUNT(Utf8)`,
`SUM(Float64)`, `MIN(UInt64)`, `MAX(Timestamp(Microsecond, None))`, a distinct or
filtered aggregate, and a test aggregate whose state or result is `UInt64` while
the accepted expression is otherwise unchanged. Also reject `Single` and
`Final` modes; a left, empty-key, expression-key, or filtered hash join; a
same-table SQL `UNION ALL`, a cross-table union, and an internal union with an
unmapped source id; and `SortExec` with `fetch() == Some(_)`. Retain missing
exchange, unknown semantic operator, and every positive matrix row. Include the
existing UTF-8 `filter_key` grouping journey as a positive case. Exact:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::exec::tests::supported_analytical_plan_accepts_only_the_v1_baseline)'
```

**GREEN.** Tighten the existing synchronous
`splitter::validate_supported` recursion to accept `source_groups` and encode the
matrix above with direct downcasts and matches in that function. For each
`AggregateFunctionExpr`, downcast `fun().inner()` to the pinned `Count`, `Sum`,
`Min`, or `Max` implementation and match `is_distinct()`, the physical argument
fields, `state_fields()`, `field()`, nullability, aggregate filter, and ordering;
do not infer support from the function name alone. Match
`HashJoinExec::join_type()`, `on()`, and `filter()` directly. Carry only the
immediate parent kind during recursion so a `UnionExec` is accepted exclusively
beneath `TenantTripwireExec`, then use the existing `source_groups` map to prove
one nonempty canonical table group. Match `SortExec::fetch()` directly. Do not
add a node registry, capability table, provenance map, trait, or second
abstraction. Reject anything outside the matrix before reservation and return
one typed support result consumed by Scenario 2 and later Task 4 without
duplicating logical optimization or estimating new routing facts.

**REFACTOR.** Keep validation synchronous, pure, and closed over semantic
capabilities while allowing pinned dependency structure to evolve within the
already-qualified plan shapes. Operator execution remains owned by DataFusion
and existing Wyrd adapters.

### Scenario 2 — Reserve and publish the immutable participant cut once before dispatch

**Behavior.** Planning, unsupported physical shapes, and no-exchange fallback
make zero reserve RPCs. After supported physical planning and irreversible
Analytical selection, reserve the complete immutable remote participant cut
once, immediately before the first stage dispatch; the dependency cannot issue
`SetPlan` or `ExecuteTask` until all reservations have succeeded. Each follower
receives exactly one reserve RPC carrying its server-generated reservation ID
and the existing signed graph/participant authority. Partial reservation failure
releases every accepted reservation. Reservation or release ambiguity fails the
query and keeps the graph supervisor-visible as `Draining` until each release
is acknowledged or reaches its retained conservative expiry. Maps REQ-004,
REQ-005, REQ-007, INV-002, INV-003, INV-004, INV-005, AC-001, AC-004.

**RED.** Add
`oracle::analytical::tests::participant_cut_is_reserved_once_immediately_before_dispatch`.
Use the existing test transport with deterministic plan, reserve, dispatch, and
release barriers. Assert zero reserve calls for planning failure, unsupported
shape, and no-exchange fallback; for a selected plan assert one reserve per
remote participant, all reserve acknowledgements before the first dispatch,
unchanged server-generated IDs in the resolver's frozen destinations, and the
two-second pending TTL. Assert the graph-owned participant-cut cell is unset
during planning and every partial reservation, resolver access fails closed
without dialing while it is unset, and the cell is set exactly once with the
complete cut before the first dispatch. Refuse participant N after earlier
acceptances and assert exact release of each accepted reservation, a failed
terminal, an unset cell, and no dispatch. Make one release acknowledgement
ambiguous and retain the follower-returned `expires_at`; assert `Draining` and
false readiness immediately before both expiry conditions, immediate clearance
after an acknowledgement, and expiry clearance only after both the returned
wall-clock expiry and one full monotonic two-second pending TTL since response
receipt. Use deterministic clock inputs or paused Tokio time, never sleeps. It
fails because reservation currently happens inside `lease_session` before
physical-plan support and exchange are known, the resolver requires the
reservation-bearing cut at session construction, and the reservation owner
discards `PendingNodeReservation::expires_at`. Use a readiness barrier to prove
orchestration remains parked until the lifecycle result reports the published
cut. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::participant_cut_is_reserved_once_immediately_before_dispatch)'
```

**GREEN.** Keep `reserve_destinations` as the sole bulk reservation operation
and `AnalyticalParticipantReservations` as its one accepted-reservation owner.
Remove reservation from session leasing. The SQL attempt orchestration first
builds and validates the distributed physical plan against the immutable cut;
unsupported or exchange-free plans take the existing pre-selection fallback
without a reservation. Once selection is final, signal the already-registered
graph lifecycle task to call `reserve_destinations` exactly once. Each graph
owns one `Arc<std::sync::OnceLock<Arc<AnalyticalParticipantCut>>>`; the resolver
retains that cell rather than a prematurely constructed participant cut.
Session planning obtains worker URLs directly from `OracleQueryAttemptCut`,
whose immutable node identities, endpoints, and fences are already available
before reservation. The lifecycle task retains the accumulated
`AnalyticalParticipantReservations` while the bulk loop runs and sets the cell
once, only after every follower accepts and the complete destination map has
been frozen with follower-minted reservation IDs. The SQL attempt orchestration
awaits the graph lifecycle's reservation-ready result before physical
execution; `AnalyticalChannelResolver::resolve` reads the cell and fails closed
without a dial when it is unset. Preserve the signed participant cut and
existing pending TTL. For every accepted reserve response, store the exact
`PendingNodeReservation::expires_at` and the local monotonic response-receipt
instant beside its existing candidate and `ReleaseNodeSlotsRequest`; do not add
a status RPC or reservation registry. Reuse the dispatcher's canonical
two-second `PENDING_TTL` for the conservative local bound rather than copying
the duration.

On partial reserve failure or terminal cleanup, the same lifecycle task calls
the existing idempotent release operation. `Ok(())` removes that exact release
record immediately. A transport or response error retains the record, fails the
query, and leaves the graph `Draining`. That record becomes authoritatively
expired only when both `Utc::now() >= expires_at` and at least `PENDING_TTL` has
elapsed on the monotonic clock since the response was received; using the later
condition prevents a leader clock ahead of the follower from releasing early.
At acknowledgement or that conservative expiry, remove the exact record. Remove
the `Draining` graph and restore readiness only after every release record has
resolved and no other cleanup failure remains; the already-failed query terminal
does not change. The graph lifecycle task owns this bounded follow-up, so no
detached timer or new protocol owner is introduced.

**REFACTOR.** Planning reads only immutable cut URLs and fences. Reservation
ownership stays graph-wide and bulk; the resolver substitutes already-issued
IDs and never reserves, derives an ID, or owns a destination state machine.

### Scenario 3 — One admitted guard, pool, runtime, and finite result transport

**Behavior.** `AnalyticalExecutionHandle::lease_session` consumes the existing
leader `AdmittedQueryGuard`; it never acquires another leader envelope. The
existing `AnalyticalSupervisor` graph owns that guard, its runtime, immutable
participant cut, reservations, cancellation tree, absolute deadline, and all
descendants until successful settlement. Each follower GraphLease owns exactly
one follower-local query envelope. Operators and exchanges on a participant use
that participant's same DataFusion pool; different queries use different pools.
Maps REQ-005, INV-005, INV-007, AC-004, AC-008.

**RED.** Add
`oracle::analytical::tests::analytical_graph_reuses_one_query_pool_per_process`.
Assert the leader admission count remains one across leasing, registration,
execution, and cleanup; follower admission is one per graph; the supervisor's
graph and the leader session expose the admitted guard's original pool identity;
operator and exchange allocations hit that pool's same current/peak counter;
and distinct queries use distinct pools. Fill the finite result channel to
prove the producer blocks/refuses without unbounded allocation, then cancel and
assert its lifecycle task joins.
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

### Scenario 4 — One attempt and terminal failure

**Behavior.** Peer, transport, protocol, resource, deadline, cancellation, or
execution failure after selection starts no successor attempt and cannot fall
back or validate preceding rows. Real peer loss and cancellation join every
reachable process and return ownership to baseline. Maps REQ-004, REQ-007,
INV-003, INV-004, INV-008, AC-001, AC-003, AC-004, AC-008.

**RED.** Add `oracle::analytical::tests::peer_loss_is_one_terminal_attempt`.
Inject peer loss before and after a data frame; assert attempt count is one,
the failure terminal invalidates all prior frames, and no retry API/counter/
state remains. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::peer_loss_is_one_terminal_attempt)'
```

Replace the retry journey in
`crates/wyrd/wyrd-testing/tests/bifrost/oracle/analytical_inactive.rs` with
`peer_network::analytical::one_attempt_peer_loss_and_cancellation_join_every_process`
in the existing Oracle process-cluster module. Use three Oracle child processes
and one Scribe child. In separate clean-cluster cases, pause a follower's real
`ExecuteTask` after its GraphLease activates and before source completion, then
(a) kill and reap that follower process or (b) cancel from the leader. Assert
one reservation/activation per addressed follower, no successor dispatch or
reservation, one failed terminal that invalidates any earlier frames, zero live
graph/attempt/lease/cache/exchange/spill ownership on every reachable child, and
clean joined shutdown. Exact:

```bash
scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::analytical::one_attempt_peer_loss_and_cancellation_join_every_process)' --run-ignored=all"
```

**GREEN.** Delete `retry_pre_egress` and all attempt-one branches. Collapse the
supervisor attempt identity to the single graph attempt while retaining typed
attempt zero where the dependency wire requires it. Route every post-selection
failure to the graph lifecycle task, then emit one failed terminal only after
that task reports successful cleanup or retained `Draining` failure. Delete
`pg_inactive_analytical_retry_drains_attempt_zero_before_attempt_one`,
`prove_bounded_retry`, and `successor_key`. Extend only the existing test-only
process-cluster control protocol with one bounded active inactive-query slot and
pause/cancel/await operations. The pause acknowledges only after the real
follower GraphLease is active and blocks before source completion; cancellation
selects on the graph cancellation token. Keep these controls out of production
listeners and APIs, and use the existing `GraphLeases`/ownership inspection and
child reaper rather than adding a second lifecycle implementation.

**REFACTOR.** Do not rename retry into recovery; a caller's later submission is
a new public query with new identity and admission.

### Scenario 5 — One supervisor-owned lifecycle task and retained cleanup failure

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
owner exactly once before graph removal and terminal success. Race success,
cancellation, and peer failure; assert the first terminal signal alone chooses
the outcome, every cloned result receiver observes that same settlement, and no
caller takes the task handle. Inject spill and reservation cleanup failures and
assert ownership is retained as `Draining` before failed result publication,
false readiness, and shutdown residue. Exact command:

```bash
mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::leader_lifecycle_task_joins_every_owner_and_retains_failure)'
```

**GREEN.** Replace the leader's undifferentiated `AnalyticalGraphState` map
value with `AnalyticalGraphEntry::{Active, Draining}`, following the existing
follower lifecycle shape. A cohesive graph lifecycle owner retains the admitted
guard, runtime, immutable Oracle cut, graph-owned participant-cut `OnceLock`,
reservation owner, cancellation tree, descendants, and cleanup evidence. Each
entry owns one pair of monotonic `tokio::sync::watch` channels: a control channel
whose private state advances from registered to reserve-requested and then to
the first terminal outcome, and a result channel whose private state advances
from pending to reservation-ready and finally settled success or failure. The
entry retains the control sender, clonable result receiver, and one
supervisor-owned `Option<JoinHandle<()>>`; the lifecycle task owns the matching
control receiver and result sender. Callers signal through the supervisor and
await cloned result receivers, never the task handle. Under the graph mutex,
the first terminal signal moves `Active` to `Draining` and wins; later terminal
signals observe `Draining`, leave the outcome unchanged, and await the same
result.

The lifecycle task alone owns the order: close dispatch; cancel on non-success;
join coordinator channels, stage drivers, returned streams, tasks, cache
invalidation, and exchanges; verify pool idle; clean and verify graph
scratch/spill; resolve follower reservations by Scenario 2's exact
acknowledgement-or-conservative-expiry rule; and
release the admitted guard. After successful cleanup it removes the graph entry
before publishing settled success. On timeout, ambiguity, or any cleanup error
it first leaves every unresolved owner in the existing `Draining` entry and
records the failure, then publishes settled failure; readiness remains false.
Normal callers never take or await the `JoinHandle`; only shutdown takes each
remaining handle and joins it after signaling settlement. A normally settled
entry drops its completed handle when it is removed. `OracleQueryStream`
terminal handling signals and awaits the result receiver. Its `Drop` path only
signals cancellation; supervisor ownership keeps the task alive and makes
shutdown join it.

**REFACTOR.** The lifecycle task is the single settlement sequence. Query
stream, shutdown, and error sites may signal or await it but may not reproduce
its order, spawn substitute cleanup, or log-and-ignore its result.

## Cross-scenario decisions and authority

The immutable Oracle query cut is selected before reservation; bulk reservation
occurs only after supported physical planning and immediately before the first
dispatch. Planning reads worker URLs from `OracleQueryAttemptCut`. The graph's
single `OnceLock` atomically publishes the complete reservation-bearing
`AnalyticalParticipantCut`; the resolver consumes it and is not a reservation
owner. Premature resolver access fails closed. One absolute deadline covers
reserve, execute, cleanup, and terminal settlement. The dependency's aggregate
byte backpressure remains the honest dependency boundary; Wyrd does not claim
an item limit.

Each accepted reservation retains the follower-minted `expires_at` plus its
leader monotonic receipt instant. An ambiguous release remains `Draining` until
acknowledged or until both the returned wall-clock expiry and the full canonical
two-second pending TTL since receipt have elapsed. This is the only expiry rule;
it needs no release-status protocol.

No new wire or public contract is required. The supervisor's existing graph map
is the authoritative lifecycle registry: `Active` accepts control until the
first terminal outcome, success removes its entry before result publication,
and cleanup ambiguity retains the same ownership as `Draining` before failed
result publication. The one lifecycle task and watch control/result pair are
per graph and stored there; there is no global queue or scheduler.

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
- Supported/unsupported physical-tree mutation evidence proving exact aggregate
  argument/state/result types, inner column equi-joins, provider-local
  same-table unions, and `SortExec` without `fetch` before reservation.
- Partial-reservation and ambiguous-release evidence proving accepted leases
  retain their returned expiry, remain `Draining` before the later wall/monotonic
  threshold, and clear only on acknowledgement or conservative expiry.
- Publication traces proving planning used `OracleQueryAttemptCut`, premature
  resolver access failed closed, the graph cell remained unset through every
  partial reservation, and the complete reservation-bearing cut was set once
  before execution.
- Admission and pool identity snapshots proving one leader admission, one
  participant-local memory pool, and finite result pressure.
- Source and telemetry proof that no automatic retry, exchange child,
  per-destination reservation machinery, or second acquisition remains.
- Three-Oracle/one-Scribe child-process evidence proving peer loss and explicit
  cancellation each run one attempt, emit one failed terminal, reap or join
  every reachable owner, and return live ownership to baseline.
- Raw stream-drop and shutdown evidence proving the supervisor-owned lifecycle
  task remains joined and visible.
- Per-outcome snapshots proving cleanup failure retains `Draining`, removes
  readiness, prevents success, and successful cleanup releases every owner
  exactly once.

## Stop conditions

Return `SPEC_REVISION_REQUIRED` if correctness requires reservation before
supported physical-plan validation, automatic retry, a separate exchange pool,
best-effort cleanup, or behavior outside the approved obligations. Return
`PLAN_BLOCKED` only if current repository or pinned dependency APIs cannot
separate supported physical planning from first dispatch, cannot build planning
worker URLs from `OracleQueryAttemptCut`, cannot transfer the admitted guard
into the existing supervisor graph, or cannot install the bounded test-only
process gate without changing a production contract. Report the exact missing
capability; do not restore the deleted lazy-reservation design or invent a new
contract.
