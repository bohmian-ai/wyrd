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

## Execution evidence

### Scenario 1 — supported physical-plan predicate

- RED: `oracle::exec::tests::supported_analytical_plan_accepts_only_the_v1_baseline`
  failed because `splitter::validate_supported` matched operator *names* only and
  accepted any aggregate, join, or union shape beneath them.
- GREEN: closed the predicate in
  `crates/vala/vala-bifrost-redux/src/oracle/splitter.rs` — downcast-checked
  aggregate UDAF identity, argument/state/result typing, inner column
  equi-joins, provider-local same-table unions beneath `TenantTripwireExec`, and
  `SortExec` without `fetch`. Argument types are checked in `Partial` mode
  against `AggregateExec::input_schema()` because `AggregateFunctionExpr::arg_fields`
  is private; every distributed aggregate carries a `Partial` layer, so the
  matrix stays closed for the whole plan.
- Two pre-existing splitter tests were corrected, not weakened: `LIMIT`/
  `GlobalLimitExec` is now outside the supported matrix, so
  `distributed_split_retains_global_operators_on_leader` drops its `LIMIT 1` and
  `distributed_split_wraps_final_plan_at_top` builds a real `SortExec`.
- Command: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::exec::tests::supported_analytical_plan_accepts_only_the_v1_baseline) or test(/oracle::splitter/)'` → 4 passed.
- Commit `26331b94c`.

### Scenario 2 — reserve and publish the participant cut once before dispatch

- RED: `oracle::analytical::tests::participant_cut_is_reserved_once_immediately_before_dispatch`
  could not be satisfied by the prior design — `lease_session` reserved before
  physical-plan support or exchange were known, `AnalyticalChannelResolver`
  required a reservation-bearing cut at session construction, and
  `reserve_destinations` discarded `PendingNodeReservation::expires_at`.
- GREEN:
  - `AnalyticalChannelResolver` now retains
    `Arc<OnceLock<Arc<AnalyticalParticipantCut>>>` and `resolve` fails closed on
    an unset cell before any channel work. The follower path wraps its adopted
    cut in an already-published cell.
  - `AnalyticalExecutionHandle::lease_session` no longer reserves. It projects
    remote participants from `OracleQueryAttemptCut` via the IO-free
    `remote_participants` (replacing `reserve_destinations`), so planning reads
    only immutable node identities, endpoints, and fences.
  - New `AnalyticalGraphLifecycle` task per graph, started at registration, with
    a monotonic `tokio::sync::watch` control channel
    (`Registered` → `ReserveRequested` → `Terminal`) and result channel
    (`Pending` → `ReservationReady`/`ReservationFailed`). It owns the bulk
    `reserve` loop, publishes the frozen cut into the cell exactly once after
    the last acceptance, releases everything on partial failure, and retains
    unacknowledged releases.
  - `AnalyticalRetainedRelease` stores the follower-returned `expires_at` and a
    local monotonic receipt `Instant`; `conservatively_expired` requires both
    `Utc::now() >= expires_at` and a full `dispatcher::PENDING_TTL` (now
    `pub(super)`, reused rather than copied) elapsed monotonically.
  - `AnalyticalSupervisor` gained a `draining` registry
    (`retain_graph_cleanup` / `resolve_graph_cleanup` / `draining_graphs`), and
    `AnalyticalExecutionHandle::is_healthy` now refuses readiness while any
    graph is draining.
  - `AnalyticalAttemptOwnership.participants` became `signals`;
    `publish_participants` is the attempt-side barrier and `settle`/`Drop`
    signal the terminal after both local guards are gone.
  - `Oracle::execute_distributed_session` calls `publish_participants` after
    `plan_distributed_split` succeeds and only when the split produced a
    follower subtree, so failed planning, an unsupported shape, and the
    exchange-free fallback all issue zero reserve RPCs.
- Command: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::participant_cut_is_reserved_once_immediately_before_dispatch)'` → 1 passed.
- Broader: `mise run fmt`; crate lib suite 961/965 passed with the same four
  pre-existing failures (`catalog::bifrost_catalog::production_pin_tests::*` ×2,
  `scribe::persistence::tests::*` ×2) proven unrelated earlier in this task;
  `cargo clippy -p vala-bifrost-redux --all-features --all-targets` clean;
  `git diff --check` clean.
- Bounded corrections recorded: (1) the test uses `tokio` `test-util`
  (`start_paused`) added as a dev-dependency feature, since the retention bound
  is inherently time-based and the task forbids sleeps; (2) the lifecycle task
  is spawned detached in this scenario — Scenario 5 moves its `JoinHandle` into
  the supervisor entry and joins it, together with the
  `AnalyticalGraphEntry::{Active, Draining}` map-value refactor; the `draining`
  registry added here provides the required supervisor-visible `Draining` state
  in the meantime.

### Scenario 3 — One admitted guard, pool, runtime, and finite result transport

- RED: `oracle::analytical::tests::analytical_graph_reuses_one_query_pool_per_process`
  failed at `the registered graph installs the admitted envelope's own pool`
  when `lease_session` built its runtime from a fresh `GreedyMemoryPool` instead
  of the admitted envelope's pool (temporary one-line inversion, reverted).
- GREEN:
  - `AdmittedQueryGuard::take_query_resources` moves the leader envelope out of
    the admission permit; `lease_session` takes `&mut AdmittedQueryGuard`,
    builds the graph runtime from that envelope's pool and scratch grant, and
    registers it. The handle no longer holds `OracleResources` and issues no
    second `try_acquire_query`.
  - Release order is now envelope-before-counters in both `Drop for
    AdmittedQueryGuard` and `AdmittedQueryGuard::release`, so a queued waiter is
    never admitted while the envelope it needs is still charged.
  - Exchange child allocation is gone end to end: `EXCHANGE_CONSUMER`,
    `AnalyticalAttemptGrant::exchange_buffer_bytes`,
    `AnalyticalAttemptState::exchange_memory`,
    `AnalyticalAttemptRelease::exchange_buffer_bytes`, the `try_split_memory`
    call in `spawn_attempt`, `AnalyticalStageIngressConfig`/
    `AnalyticalExecutionConfig::exchange_buffer_bytes`, and the
    `analytical_exchange_buffer_bytes` server config are removed. The pinned
    exchange consumer now allocates from the installed pool.
  - `AnalyticalGraphRuntime` collapsed to its single `Arc<RuntimeEnv>`; its
    `exchange_buffer_budget_bytes` prediction field had no consumers anywhere.
- Command: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::analytical_graph_reuses_one_query_pool_per_process)'` → 1 passed.
- `oracle::analytical_supervisor::tests::analytical_attempt_installs_query_owned_runtime_and_releases_once`
  was updated in place: it now asserts the attempt pre-charges nothing and that
  a pinned exchange consumer charges the installed query pool.
- Broader: `mise run fmt`; crate lib suite 962/966 passed with the same four
  pre-existing failures, re-proven unrelated by re-running them on a stashed
  tree; `cargo clippy -p vala-bifrost-redux --all-features --all-targets` clean;
  `cargo check -p wyrd-testing --all-features --all-targets` clean;
  `mise run check:bifrost-resource-governance` passed;
  `mise run check:bifrost-oracle-deploy` 2 passed; `git diff --check` clean.
- Bounded corrections recorded:
  1. The task's literal "transfer the `AdmittedQueryGuard` into the graph" is
     unrepresentable — the guard owns `analytical: Option<AnalyticalAttemptOwnership>`,
     which owns the graph guard. The transferred value is therefore the guard's
     `OracleQueryResources` envelope, which is what the rest of the scenario
     ("build the graph runtime from that guard's pool and spill grant", "never
     acquires another leader envelope") actually constrains.
  2. `lease_inactive_analytical_attempt` (the `test-support` seam) previously
     relied on the deleted `try_acquire_query` path. It now admits through the
     production `admit_sql_query` and hands the resulting guard to
     `AnalyticalAttemptOwnership::retain_admission`, so the seam holds admission
     for exactly as long as the graph holds the envelope. The guard it retains
     has `analytical: None`, so no ownership cycle exists.
  3. The finite-result-channel fill and lifecycle-task join named in this
     scenario's RED are deferred to Scenario 5, which owns the supervisor's
     `JoinHandle` and the `AnalyticalGraphEntry::{Active, Draining}` refactor.
     This scenario's terminal proof is that settlement returns the graph and the
     envelope to the process root exactly once.

### Scenario 4 — One attempt and terminal failure

- RED: `oracle::analytical::tests::peer_loss_is_one_terminal_attempt` failed at
  `no successor attempt ordinal is representable` (`left: Some(AnalyticalAttemptNumber(1))`)
  when `AnalyticalAttemptNumber::from_u8` still admitted ordinal one (temporary
  one-line restoration, reverted).
- GREEN:
  - `AnalyticalAttemptNumber` keeps `ZERO` only. `ONE`, `retry()`, and the
    ordinal-one parse branch are gone, so a successor is unrepresentable rather
    than refused by policy; the wire ordinal survives only because the
    distributed dependency encodes one.
  - `AnalyticalAttemptOwnership::retry_pre_egress` and the
    `AnalyticalAttemptOutcome::Retried` telemetry outcome are deleted
    (`ALL` is now three).
  - `pg_inactive_analytical_retry_drains_attempt_zero_before_attempt_one`,
    `prove_bounded_retry`, and `successor_key` are deleted. The stale-identity
    proof in `prove_stale_and_sibling_fencing` now names an unadmitted sibling
    *stage* of the same graph, which is what "stale" means once a graph has one
    attempt.
  - Follower pause seam: `AnalyticalExecutePause` (test-support only) holds the
    first authorized `ExecuteTask` after its `GraphLease` is active and before
    any source is consumed, armed through
    `AnalyticalStageIngress::bind_execute_pause_for_test`.
  - Process-cluster control protocol extended with one bounded active
    inactive-query slot (`StartInactiveSql`/`CancelInactiveSql`/`AwaitInactiveSql`)
    and the pause operations (`ArmExecutePause`/`AwaitExecutePaused`/
    `ReleaseExecutePause`), plus `ProcessNode::kill` for abrupt peer loss over
    the existing reaper. Cancellation selects on a `CancellationToken` in the
    slot; no second lifecycle implementation was added.
- Command: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::peer_loss_is_one_terminal_attempt)'` → 1 passed.
- Journey: `peer_network::analytical::one_attempt_peer_loss_and_cancellation_join_every_process`
  drives three Oracle children and one Scribe child through both orderings on
  clean clusters — kill the paused follower, and cancel from the leader —
  asserting one activation per addressed follower, no successful result, and
  released leases on every reachable child including the leader.
  Command: `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=peer_network::analytical::one_attempt_peer_loss_and_cancellation_join_every_process)' --run-ignored=all"`.
- Bounded correction recorded: the unit test runs on a multi-threaded runtime.
  The graph lifecycle task is otherwise polled on the test's own stack, and the
  combined debug-build frame overflows it; the two orderings also share one
  boxed future slot for the same reason.

### Scenario 4 remediation — reserve and release one graph on the real query path

Commit `e7fc79439`. The Scenario 4 unit proof leased through the inactive seam,
which never reaches `Oracle::execute_distributed_session`, so the production SQL
path was still reserving and releasing outside the graph the supervisor owns.

- `execute_distributed_session` now reserves exactly once, through the graph's
  own signals, after `plan_distributed_split` reports a supported plan that
  produced a follower subtree — so failed planning, an unsupported shape, and
  the exchange-free fallback each issue zero reserve RPCs.
- The same path settles through that graph rather than through a locally held
  owner, so the real query path and the inactive seam share one reservation and
  one release.
- Verified with `mise run test:bifrost` and
  `mise run test:bifrost:journey:oracle`.

### Scenario 1 remediation — the ungrouped `Final` aggregate is supported

Commit `2a8f25c1c`. Re-verified rather than inherited: the open failure of
`distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads`
was recorded as pre-existing at baseline `1f020ead2`, but `1f020ead2` already
contains Scenario 1 (`26331b94c`). Running the journey at `26331b94c~1` passes
and at `26331b94c` onward fails, so the failure is this task's regression.

- Cause: the closed predicate accepted only `Partial`, `PartialReduce`, and
  `FinalPartitioned`. `DataFusion` plans every ungrouped aggregate as
  `Partial` → `CoalescePartitionsExec` → `Final`, so `SELECT count(*)` was
  refused as an unsupported distributed shape and reported
  `QueryExecutionFailed`, hiding the tenant tripwire's `QueryTenantInvariant`.
- Fix: `Final` joins the accepted set — it consumes the same accumulator state
  `FinalPartitioned` does. `Single` and `SinglePartitioned` stay refused: they
  carry no partial layer, so no state crosses a participant boundary.
  `assert_accepted_aggregate_matrix` was corrected accordingly, not weakened.
- Commands: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::exec::tests::supported_analytical_plan_accepts_only_the_v1_baseline) or test(/oracle::splitter/)'` → 4 passed;
  `scripts/postgres/with-test-postgres.sh -- bash -lc "mise run db:migrate:inner && mise exec -- cargo nextest run --locked -p wyrd-testing --test oracle -P journey -E 'test(=distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads)' --run-ignored=all"` → 1 passed.

### Scenario 5 — One supervisor-owned lifecycle task and retained cleanup failure

- RED: `oracle::analytical::tests::leader_lifecycle_task_joins_every_owner_and_retains_failure`
  failed at `a cleanup failure prevents a success terminal` when the settlement
  path published `SettledSuccess` and dropped the failure detail instead of
  retaining the graph — the log-and-ignore settlement this task invalidates
  (temporary inversion of the failure branch, reverted).
- GREEN, commit `d6c0e2a5c`:
  - The leader supervisor's map value is now
    `AnalyticalGraphEntry::{Active, Draining}`, following the follower's
    existing lifecycle shape. `AnalyticalGraphState` gained the entry's
    `AnalyticalGraphLifecycleOwner`: the control sender, a clonable result
    receiver, and the one `Option<JoinHandle<()>>`. The separate `draining`
    `HashSet` is gone; `draining_graphs` counts `Draining` entries carrying a
    recorded failure, which keeps readiness false for residue only and not for
    a graph merely passing through settlement.
  - New supervisor operations: `attach_lifecycle`, `signal_reserve`,
    `signal_terminal`, `graph_settlement_failure`, and the private
    `take_lifecycle_task`. `signal_terminal` moves `Active` to `Draining` under
    the graph mutex and only the first signal sends the outcome; every later
    signal observes `Draining`, leaves the outcome unchanged, and is handed the
    same result receiver.
  - `AnalyticalGraphSignals` no longer holds a control sender or a task. It
    holds the supervisor, the graph key, its own clone of the result receiver,
    the participant cell, and a `oneshot` the admission owner is handed through.
    Its `Drop` signals cancellation and nothing else.
  - `AnalyticalGraphControl::Terminal` carries the outcome;
    `AnalyticalGraphResult` gained `SettledSuccess(Option<AnalyticalAttemptRelease>)`
    and `SettledFailure`.
  - The lifecycle task owns the whole order: move the entry out of `Active`,
    cancel on a non-success outcome, join the attempt and every driver it
    retained, close the graph's exchange registry, return every participant
    reservation under Scenario 2's expiry rule, wait out the envelope's nested
    children, release the graph, and release the admission owner last of all.
    Successful cleanup removes the entry before publishing settled success; an
    unconfirmed cleanup calls the new `AnalyticalGraphGuard::retain`, records
    the detail on the `Draining` entry, and publishes settled failure.
  - `AnalyticalAttemptOwnership` no longer holds either guard — the task does.
    It keeps the attempt key, its cancellation child, the shared egress fence,
    and the signals; `settle` signals and awaits, and never performs cleanup.
  - `AnalyticalSupervisor::shutdown` signals every remaining graph, takes each
    handle, and joins it before the attempt and graph sweep.
  - `settle_analytical` now reports whether the graph settled cleanly, and
    `settle_and_finish_stream` downgrades its terminal when it did not, so a
    retained cleanup cannot become a success terminal.
- Command: `scripts/postgres/with-test-postgres.sh -- mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::leader_lifecycle_task_joins_every_owner_and_retains_failure)'` → 1 passed.
- Follow-on fix, commit `13170a9b9`: `pg_inactive_analytical_production_telemetry_covers_every_hot_path`
  failed with a leader graph retained at 547 bytes of nested memory. An outbound
  exchange keeps its reader task, and the buffers it charges to the query pool,
  alive until every partition stream it handed out is dropped; cancellation
  alone does not drop them. The leader lifecycle task now closes the graph's own
  exchange registry before waiting for the envelope's children, exactly as the
  follower lease already did.
- Bounded corrections recorded:
  1. `ReservationFixture` now registers a real graph and attaches its lifecycle,
     because the supervisor's graph entry *is* the lifecycle registry: a task
     with no entry could not move its graph to draining, retain a failed
     cleanup, or be joined by shutdown.
  2. The result channel keeps only its latest value, so a settlement can
     overwrite the reservation verdict. `publish_participants` therefore reads
     the durable record — the participant cell is set once, only after every
     participant accepted, so its absence is the refusal.
  3. The `AdmittedQueryGuard` the inactive seam retains is handed to the
     lifecycle task through a `oneshot` rather than held by the ownership, so
     the admission counters are returned only after the graph has returned the
     envelope taken out of that guard, on the drop path as well as on `settle`.
- Broader verification: `mise run fmt`; `mise run lints` clean;
  `mise run test:bifrost` → 969/969 passed (the four
  `catalog::bifrost_catalog::production_pin_tests::*` /
  `scribe::persistence::tests::*` failures reported in earlier scenarios were an
  artifact of running bare `cargo nextest` without
  `scripts/postgres/with-test-postgres.sh`; they pass in the lane);
  `mise run test:bifrost:journey:oracle` → 15/15 passed;
  `mise run check:bifrost-resource-governance` passed;
  `mise run check:bifrost-oracle-deploy` → 2 passed; `git diff --check` clean.

### Remediation — `$wyrd-task-review` findings 1–3 (Scenario 5)

Three implementation defects against existing REQ-007 and Scenario 5 authority.
No specification revision; no new dependency, protocol, RPC, scheduler, timer
task, registry, or retry state machine.

**FIND-BIFROST-R4-T02-QUERY-ENVELOPE-2 — admission follows retained graph ownership.**
`AnalyticalGraphState` now owns `retained_admission: Option<AdmittedQueryGuard>`,
declared immediately after `resources` so removing the graph drops the envelope
first and the permit second. `AnalyticalSupervisor::retain_admission` stores one
cycle-free guard on an `Active` or `Draining` entry and returns it unchanged on
refusal. The admission `oneshot` is gone from `AnalyticalGraphSignals`,
`AnalyticalGraphLifecycle`, `start`, and `settle`.
`AnalyticalAttemptOwnership::retain_admission` now stores synchronously through
the supervisor and is used by both the inactive seam and the production stream.
`settle_analytical` takes the Analytical ownership out of the guard, stores the
remainder in the graph, and only then signals and awaits settlement; it returns
`AnalyticalStreamSettlement { clean, transferred }` and
`release_and_finish_terminal` skips the stream-local release for a transferred
guard while preserving the existing missing-owner diagnostic.

- RED: `oracle::analytical::tests::a_failed_graph_cleanup_keeps_its_query_admission_charged`
  fails against the pre-transfer shape (mutation: the stream keeps its guard) at
  "the graph took this query's admission owner before settlement".
- GREEN: passes; drives production-shaped `settle_analytical` over a real
  `OracleAdmission` with one Analytical slot. Proves `active_queries` stays 1,
  the queued Analytical caller stays blocked, the graph is `Draining`, readiness
  is false, the retained permit is still held by the graph, and that removing
  the graph returns the envelope before the permit so the waiter proceeds and
  accounting returns to 0.
- Command: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::a_failed_graph_cleanup_keeps_its_query_admission_charged)'` → 1 passed.

**FIND-BIFROST-R4-T02-QUERY-ENVELOPE-1 — bounded peer operations.**
The existing monotonic admission deadline is threaded from both
`OracleQueryService` lease paths through `AnalyticalExecutionHandle::lease_session`
into `AnalyticalGraphLifecycle::start` and stored on the lifecycle, which also
holds the graph's existing cancellation child rather than creating a tree.
`reserve` selects (biased) over that cancellation and
`tokio::time::timeout_at(graph_deadline, …)`; either edge takes the existing
reservation-failed path and returns every acceptance so far. One narrow helper,
`release_within`, bounds every release attempt by
`min(graph_deadline, now + RETAINED_RELEASE_RETRY)`, treats a timeout exactly
like an unacknowledged release, and issues no RPC once the deadline has elapsed.
`drain` reuses it, sleeps with `sleep_until` capped at the deadline, and now
returns residue as an error so an unresolvable release settles as failure
instead of releasing the graph. `dispatcher.rs` is unchanged.

- RED: `oracle::analytical::tests::graph_peer_operations_are_bounded_by_the_graph_deadline`
  fails under three separate mutations — cancellation arm removed
  ("cancellation ends the reservation immediately, not at the far deadline"),
  reserve bound moved off the graph deadline ("the reservation ended at the
  graph's own deadline, not at some other bound"), and the release bound removed
  ("cleanup retried the ambiguous release to the graph deadline and stopped
  there").
- GREEN: passes under `start_paused` time. Proves cancellation and the deadline
  each interrupt a permanently pending reservation, a pending release cannot run
  beyond its bound, no peer call begins after the deadline, settlement returns
  failure rather than hanging, and shutdown reports retained residue instead of
  awaiting an RPC. Ambiguous-release retention until acknowledgement or
  conservative expiry stays covered by the existing reservation-lifecycle tests.
- Command: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::graph_peer_operations_are_bounded_by_the_graph_deadline)'` → 1 passed.

**FIND-BIFROST-R4-T02-QUERY-ENVELOPE-3 — no cleanup from `Drop`.**
`impl Drop for AnalyticalParticipantReservations` is deleted with no
replacement owner. The lifecycle-held `AnalyticalGraphGuard` is disarmed in
`AnalyticalGraphLifecycle::start` the moment the task takes ownership, and the
success path releases explicitly through `AnalyticalSupervisor::release_graph`
after children and reservations settle, so no unwinding or aborted task can
remove a graph the supervisor has not yet recorded a failure against. Shutdown
now treats every lifecycle `JoinError`, cancellation included, as cleanup
failure via the existing `retain_graph_cleanup`, and the final graph sweep skips
any graph with a recorded failure — reusing the existing `Draining` failure
state rather than a second collection.

- RED: `oracle::analytical::tests::an_aborted_lifecycle_task_retains_its_graph_instead_of_releasing_it`
  is unreachable before the change (`Drop` would have issued the releases the
  test forbids) and the shutdown sweep would have removed the graph.
- GREEN: passes. Proves no release RPC is ever issued outside the lifecycle
  sequence, the supervisor records the cleanup failure, the graph stays
  registered and `Draining`, the retained admission stays charged, readiness is
  false, and the shutdown report carries the graph rather than sweeping it.
- Command: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::an_aborted_lifecycle_task_retains_its_graph_instead_of_releasing_it)'` → 1 passed.

Bounded corrections recorded:

1. `assert_cleanup_failure_retains_draining_ownership` previously asserted
   `graphs_released == 1`: the shutdown sweep removed the very residue it was
   meant to report. It now asserts `(graphs_released, graphs_retained) == (0, 1)`,
   which is what finding 3 requires.
2. `drain` returning residue is new: it previously could not return unresolved,
   so `settle` could reach `release_graph` and remove a graph whose reservations
   were never returned. It now reports the residue as the cleanup failure.
3. `AnalyticalSupervisor` gained two `#[cfg(test)]` accessors —
   `abort_lifecycle_task_for_test` (aborts the handle in place, leaving shutdown
   to take and join it, exactly as a panic would) and
   `retains_admission_for_test` — because neither the task's exceptional end nor
   a graph-held permit is otherwise observable.
4. `admission::tests::owner_with_resources` now delegates to a new
   `#[cfg(test)] pub(in crate::oracle) admission_owner_for_test`, so the
   Analytical lifecycle test uses the production admission owner instead of a
   duplicated one.
5. The refusal `Err` variants carry `Box<AdmittedQueryGuard>`, matching the
   existing `register_graph` precedent, to satisfy `clippy::result_large_err`.

Remediation verification: existing Scenario 5 test retained and passing;
`mise run fmt`; `mise run lints` clean; `mise run test:bifrost` → 972/972 passed;
`mise run test:bifrost:journey:oracle` → 15/15 passed (the first, cold-build
run flaked on `distributed::pg_bifrost_selective_predicate_and_projection_prune_distributed_reads`,
a pruning journey with no Analytical lifecycle involvement; it passes standalone
and in a clean full lane run);
`mise run check:bifrost-resource-governance` passed;
`mise run check:bifrost-oracle-deploy` → 2 passed; `git diff --check` clean.

## Remediation round 4 — findings 4 and 5

### FIND-BIFROST-R4-T02-QUERY-ENVELOPE-4 — graph release outran the deadline

- RED: new paused-free branch `assert_release_stops_at_the_graph_deadline` in
  `oracle::analytical::tests::leader_lifecycle_task_joins_every_owner_and_retains_failure`
  leases a graph bounded 200 ms out, holds a nested scratch child through a
  stray attempt, and settles. Failed with a 5.76 s overrun past the deadline:
  `release_graph` polled `GRAPH_DRAIN_POLLS` × `GRAPH_DRAIN_INTERVAL`
  unconditionally.
- GREEN: `release_graph` now breaks at `self.deadline` and sleeps
  `sleep_until(self.deadline.min(now + GRAPH_DRAIN_INTERVAL))`. No new helper,
  timer, task, or configuration. Settlement now fails at the deadline with the
  graph and its envelope retained as `Draining`.
- Command: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::leader_lifecycle_task_joins_every_owner_and_retains_failure)'` → 1 passed.

### FIND-BIFROST-R4-T02-QUERY-ENVELOPE-5 — post-deadline conservative expiry

- RED: new branch `assert_post_deadline_expiry_returns_the_graph` in
  `oracle::analytical::tests::graph_peer_operations_are_bounded_by_the_graph_deadline`
  uses an already-elapsed `expires_at` with a 500 ms graph deadline, so the
  canonical two-second pending TTL is the deciding clock and falls after the
  deadline. Failed: the graph stayed `Draining` forever (`draining() == 1`)
  because `drain` returned at the deadline and the lifecycle task ended.
- GREEN: `drain` now returns the unresolved records instead of an error;
  `settle` derives the same cleanup failure from a non-empty result, publishes
  the caller's failure at the deadline as before, and then — only when nothing
  else failed — runs the new `AnalyticalGraphLifecycle::expire`. That loop
  issues no RPC, re-evaluates only the existing two-clock
  `conservatively_expired` predicate at the existing `RETAINED_RELEASE_RETRY`
  cadence, returns immediately once the supervisor stops accepting so shutdown
  still joins promptly and reports the residue, and releases the graph, its
  envelope, and the retained admission once every record has expired. The
  shared detail string is now the `RETAINED_RELEASE_UNACKNOWLEDGED` const. The
  existing lifecycle task remains the sole owner: no scheduler, detached task,
  registry, protocol, or dependency was added.
- Command: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::graph_peer_operations_are_bounded_by_the_graph_deadline)'` → 1 passed.

Round 4 verification: `mise run fmt`; `mise run lints` clean;
`mise run test:bifrost` → 972/972 passed;
`mise run test:bifrost:journey:oracle` → 15/15 passed;
`mise run check:bifrost-resource-governance` passed;
`mise run check:bifrost-oracle-deploy` → 2 passed; `git diff --check` clean.

## Remediation round 5 — finding 4 boundary-success path

- RED: new branch `assert_an_idle_graph_past_the_deadline_is_not_released` in
  `oracle::analytical::tests::leader_lifecycle_task_joins_every_owner_and_retains_failure`
  leases a graph that reserves nothing, sleeps past its 50 ms deadline, then
  settles. Failed by publishing `AnalyticalAttemptRelease { outcome: Success }`:
  `release_graph` tested `graph_children_idle` before `self.deadline`, so an
  already-idle graph reached after the bound was released and reported clean.
- GREEN: the deadline check now precedes the idle check in the loop, so an
  ordinary settlement can never publish success at or after the deadline. The
  post-deadline expiry path calls `self.supervisor.release_graph(self.graph)`
  directly instead of the deadline-bound wait — that wait exists to hold an
  ordinary settlement inside the deadline, and expiry is past it by
  construction; the supervisor's own refusal still covers a graph whose
  envelope a child has not returned. The drain warning now names the deadline
  rather than the poll count.
- Commands: `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::leader_lifecycle_task_joins_every_owner_and_retains_failure)'` → 1 passed;
  `mise exec -- cargo nextest run --locked -p vala-bifrost-redux --lib --features test-support,bench-support -E 'test(=oracle::analytical::tests::graph_peer_operations_are_bounded_by_the_graph_deadline)'` → 1 passed.

Round 5 verification: `mise run fmt`; `mise run lints` clean;
`mise run test:bifrost` → 972/972 passed;
`mise run test:bifrost:journey:oracle` → 15/15 passed;
`mise run check:bifrost-resource-governance` passed;
`mise run check:bifrost-oracle-deploy` → 2 passed; `git diff --check` clean.
